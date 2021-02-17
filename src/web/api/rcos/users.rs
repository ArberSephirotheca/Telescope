//! API interactions for RCOS users from the central RCOS API.

use crate::models::users::User;
use crate::error::TelescopeError;
use crate::web::api::rcos::{
    auth::*,
    api_endpoint
};
use actix_web::client::Client;
use actix_web::http::StatusCode;
use crate::models::parameters::QueryParameters;
use crate::models::parameters::filter::{FilterParameterRepr, ComparisonOperator};
use crate::models::parameters::pagination::PaginationParameter;

/// The path on the API endpoint for the user table.
const USER_PATH: &'static str = "users";

/// Add a user to the central RCOS database via the API.
pub async fn create_user(user: User) -> Result<(), TelescopeError> {
    // Create the http client to communicate with the central RCOS API.
    let http_client: Client = make_client(AUTHENTICATED_USER, ACCEPT_JSON);

    info!("Adding user to database: {}", user.username);

    // Send the request.
    let response = http_client
        .post(format!("{}/{}", api_endpoint(), USER_PATH))
        .send_json(&user)
        .await
        // Convert and propagate any errors.
        .map_err(TelescopeError::api_query_error)?;

    // Check the status code.
    if response.status() != StatusCode::CREATED {
        return Err(TelescopeError::ise("Could not add new user to the central RCOS database. \
        Please contact a coordinator and file a GitHub issue."));
    }
    // Otherwise we were successful in creating a user.
    Ok(())
}

/// Try to get a user from the database by their username
pub async fn get_by_username(username: impl Into<String>) -> Result<Option<User>, TelescopeError> {
    // Make an http client.
    let http_client: Client = make_client(AUTHENTICATED_USER, ACCEPT_JSON);

    // Convert the username.
    let username: String = username.into();

    info!("Finding user by username: {}", username);

    // Construct query parameters.
    let params: QueryParameters = QueryParameters {
        filter: Some(FilterParameterRepr::comparison(
            "username".into(),
            ComparisonOperator::Equal,
            username).into()),
        pagination: Some(PaginationParameter {
            limit: Some(1),
            offset: 0
        }),
        .. QueryParameters::default()
    };

    // Format the URL to query.
    let url: String = format!("{}/{}?{}", api_endpoint(), USER_PATH, params.url_encoded());
    info!("Querying API at {}", url);

    let user: Option<User> = http_client
        // Send request with query parameter for username filter.
        .get(url)
        .send()
        .await
        // Catch and propagate any errors.
        .map_err(TelescopeError::api_query_error)?
        // Convert to a list of users.
        .json::<Vec<User>>()
        .await
        // Catch and propagate errors.
        .map_err(TelescopeError::api_response_error)?
        // The list should have one item if any.
        .into_iter()
        .next();

    return Ok(user);
}


