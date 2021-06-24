//! Creation form and services for meetings.
//!
//! The meeting creation flow is to first direct the user to pick a host,
//! or specify no host. This gets its own page, since it involves searching through
//! all users. Once the meeting creator has made a decision, they are directed to a form
//! to finish meeting creation.

use crate::api::rcos::meetings::authorization_for::AuthorizationFor;
use crate::api::rcos::meetings::creation;
use crate::api::rcos::meetings::creation::create::CreateMeeting;
use crate::api::rcos::meetings::creation::host_selection::HostSelection;
use crate::api::rcos::meetings::{MeetingType, ALL_MEETING_TYPES};
use crate::error::TelescopeError;
use crate::templates::forms::FormTemplate;
use crate::templates::Template;
use crate::web::middlewares::authorization::{Authorization, AuthorizationResult};
use actix_web::http::header::LOCATION;
use actix_web::web as aweb;
use actix_web::web::{Form, Query, ServiceConfig};
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use futures::future::LocalBoxFuture;
use serde_json::Value;

/// Authorization function for meeting creation.
fn meeting_creation_authorization(
    username: String,
) -> LocalBoxFuture<'static, AuthorizationResult> {
    Box::pin(async move {
        // Get the meeting authorization
        AuthorizationFor::get(Some(username))
            .await?
            .can_create_meetings()
            // On true, Ok(())
            .then(|| ())
            // Otherwise forbidden
            .ok_or(TelescopeError::Forbidden)
    })
}

/// Register meeting creation services.
pub fn register(config: &mut ServiceConfig) {
    // Create meeting creation auth middleware.
    let authorization = Authorization::new(meeting_creation_authorization);

    config.service(
        aweb::scope("/meeting/create")
            .wrap(authorization)
            .service(host_selection_page)
            .service(finish)
            .service(submit_meeting),
    );
}

/// Query on the host selection page.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct HostSelectionQuery {
    search: String,
}

/// Page to select a host for a meeting creation.
/// Authorized to meeting creation perms.
#[get("/select_host")]
async fn host_selection_page(
    req: HttpRequest,
    query: Option<Query<HostSelectionQuery>>,
) -> Result<Template, TelescopeError> {
    // Extract the query parameter.
    let search: Option<String> = query.map(|q| q.search.clone());
    // Query the RCOS API for host selection data.
    let data = HostSelection::get(search.clone()).await?;

    // Make and return a template.
    Template::new("meetings/creation/host_selection")
        .field("search", search)
        .field("data", data)
        .render_into_page(&req, "Select Host")
        .await
}

/// Query on finish meeting page.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FinishQuery {
    host: String,
}

/// Create an empty instance of the form to finish meeting creation.
async fn finish_form(host_username: Option<String>) -> Result<FormTemplate, TelescopeError> {
    // Query RCOS API for meeting creation context.
    let context: Value = creation::context::get_context(host_username).await?;

    // Create form.
    let mut form = FormTemplate::new("meetings/creation/forms/finish", "Create Meeting");

    // Add context to form.
    form.template = json!({
        "context": context,
        "meeting_types": &ALL_MEETING_TYPES
    });

    // Return form with context.
    return Ok(form);
}

/// Endpoint to finish meeting creation.
#[get("/finish")]
async fn finish(query: Option<Query<FinishQuery>>) -> Result<FormTemplate, TelescopeError> {
    // Extract query parameter.
    let host: Option<String> = query.map(|q| q.host.clone());
    // Return form.
    return finish_form(host).await;
}

/// Form submitted by users to create meeting.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct FinishForm {
    /// Selected semester ID.
    semester: String,

    /// What type of meeting is being created.
    kind: MeetingType,

    /// The optional meeting title. Default empty.
    #[serde(default)]
    title: String,

    start_date: NaiveDate,

    /// Cannot be a [`chrono::NaiveTime`], since seconds are not included.
    start_time: String,

    end_date: NaiveDate,

    /// Cannot be a [`chrono::NaiveTime`], since seconds are not included.
    end_time: String,

    /// The markdown description of the meeting. Default empty.
    #[serde(default)]
    description: String,

    #[serde(default)]
    is_remote: Option<bool>,

    #[serde(default)]
    meeting_url: Option<String>,

    #[serde(default)]
    location: Option<String>,

    #[serde(default)]
    recording_url: Option<String>,

    #[serde(default)]
    external_slides_url: Option<String>,

    #[serde(default)]
    is_draft: Option<bool>,
}

/// Endpoint that users submit meeting creation forms to.
#[post("/finish")]
async fn submit_meeting(
    query: Option<Query<FinishQuery>>,
    Form(form): Form<FinishForm>,
) -> Result<HttpResponse, TelescopeError> {
    // Resolve host username.
    let host: Option<String> = query.map(|q| q.host.clone());

    // Create a form instance to send back to the user if the one they submitted was invalid.
    let mut return_form: FormTemplate = finish_form(host.clone()).await?;
    // Add previously selected fields to the form.
    return_form.template["selections"] = json!(&form);

    // Validate form fields.
    // Start by destructuring form:
    let FinishForm {
        semester,
        kind,
        title,
        start_date,
        start_time,
        end_date,
        end_time,
        description,
        is_remote,
        meeting_url,
        location,
        recording_url,
        external_slides_url,
        is_draft,
    } = form;

    // We assume that semester_id is valid, since it includes only options from the creation
    // context. If it is not valid, the API will throw a foreign key constraint error on
    // meeting creation and we will return it straight to the user. This should not happen
    // if the user is using the web interface, and if they are not then the consequences are not
    // to severe, so we accept that behavior.
    //
    // TL;DR: Semester ID validation is handled client side and enforced enough API side that we
    // don't touch it here.
    //
    // Same thing with meeting type variant and host username.

    // The title should be null (Option::None) if it is all whitespace or empty.
    // If it is, we don't bother user for this -- they can change the title later and
    // they know if they put in all whitespace. This also decreases form resubmission
    // and template complexity.
    let title: Option<String> = (!title.trim().is_empty()).then(|| title);
    return_form.template["selections"]["title"] = json!(&title);

    // Check that the start date and end dates are during the semester selected.
    let selected_semester: &Value = return_form.template["context"]["available_semesters"]
        // This should be a JSON array
        .as_array()
        .expect("This value should be set as an array")
        // Find by semester ID.
        .iter()
        .find(|available_semester| available_semester["semester_id"] == semester.as_str())
        // If the submitted semester is not an available one, return an error.
        .ok_or(TelescopeError::BadRequest {
            header: "Malformed Meeting Creation Form".into(),
            message: "Could not find selected semester ID in meeting creation context.".into(),
            show_status_code: false,
        })?;

    let semester_start = selected_semester["start_date"]
        .as_str()
        .and_then(|string| string.parse::<NaiveDate>().ok())
        .expect("Semester from context has good start date.");

    let semester_end = selected_semester["end_date"]
        .as_str()
        .and_then(|string| string.parse::<NaiveDate>().ok())
        .expect("Semester from context has good end date.");

    // If meeting starts before semester, save to issues and return form.
    if start_date < semester_start {
        return_form.template["issues"]["start_date"] =
            json!("Start date is before the semester starts.");
        return Err(TelescopeError::invalid_form(&return_form));
    }

    // Same if meeting starts after the end of the semester.
    if start_date > semester_end {
        return_form.template["issues"]["start_date"] =
            json!("Start date is after the semester ends.");
        return Err(TelescopeError::invalid_form(&return_form));
    }

    // Same with end date.
    if end_date < semester_start {
        return_form.template["issues"]["end_date"] =
            json!("End date is before the semester starts.");
        return Err(TelescopeError::invalid_form(&return_form));
    }

    if end_date > semester_end {
        return_form.template["issues"]["end_date"] = json!("End date is after the semester ends.");
        return Err(TelescopeError::invalid_form(&return_form));
    }

    // Also check if the end is before the start.
    if end_date < start_date {
        return_form.template["issues"]["end_date"] = json!("End date is before start date.");
        return Err(TelescopeError::invalid_form(&return_form));
    }

    // Dates are validated, let's check the times. Start by converting the times from strings.
    let start_time: NaiveTime = format!("{}:00", start_time)
        .parse::<NaiveTime>()
        .map_err(|e| TelescopeError::BadRequest {
            header: "Malformed Meeting Creation Form".into(),
            message: format!("Could not parse start time. Internal error: {}", e),
            show_status_code: false,
        })?;

    let end_time: NaiveTime = format!("{}:00", end_time)
        .parse::<NaiveTime>()
        .map_err(|e| TelescopeError::BadRequest {
            header: "Malformed Meeting Creation Form".into(),
            message: format!("Could not parse end time. Internal error: {}", e),
            show_status_code: false,
        })?;

    // Now combine them with the dates.
    let start: NaiveDateTime = start_date.and_time(start_time);
    let end: NaiveDateTime = end_date.and_time(end_time);

    // Check the ordering.
    if start > end {
        return_form.template["issues"]["end_time"] = json!("End time is before start time.");
        return Err(TelescopeError::invalid_form(&return_form));
    }

    // Ascribe local timezone.
    let start: DateTime<Local> = Local
        .from_local_datetime(&start)
        // Expect that there is only one valid local time for this.
        .single()
        .ok_or(TelescopeError::BadRequest {
            header: "Malformed Meeting Creation Form".into(),
            message: "Could not ascribe local timezone to start timestamp.".into(),
            show_status_code: false,
        })?;

    let end: DateTime<Local> = Local
        .from_local_datetime(&end)
        // Expect that there is only one valid local time for this.
        .single()
        .ok_or(TelescopeError::BadRequest {
            header: "Malformed Meeting Creation Form".into(),
            message: "Could not ascribe local timezone to end timestamp.".into(),
            show_status_code: false,
        })?;

    // The rest of the fields are managed pretty tersely in the API call and do not need validation
    // or feedback.
    let created_meeting_id: i64 = CreateMeeting::execute(
        host,
        title,
        start.with_timezone(&Utc),
        end.with_timezone(&Utc),
        description.trim().to_string(),
        is_draft.unwrap_or(false),
        is_remote.unwrap_or(false),
        location.and_then(|string| (!string.trim().is_empty()).then(|| string.trim().to_string())),
        meeting_url,
        recording_url,
        external_slides_url,
        semester,
        kind,
    )
    .await?
    .ok_or(TelescopeError::ise(
        "Meeting creation call did not return ID.",
    ))?;

    // Redirect the user to the page for the meeting they created.
    return Ok(HttpResponse::Found()
        .header(LOCATION, format!("/meeting/{}", created_meeting_id))
        .finish());
}
