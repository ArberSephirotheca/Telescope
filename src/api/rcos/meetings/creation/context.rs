//! GraphQL query to get context for meeting creation.

use crate::api::rcos::prelude::*;
use crate::api::rcos::send_query;
use crate::error::TelescopeError;

/// ZST representing the GraphQL query to resolve meeting creation context.
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/rcos/schema.json",
    query_path = "graphql/rcos/meetings/creation/context.graphql",
    response_derives = "Debug,Clone,Serialize"
)]
pub struct MeetingCreationContext;

use meeting_creation_context::{ResponseData, Variables};

impl MeetingCreationContext {
    /// Get the meeting creation context.
    pub async fn get() -> Result<ResponseData, TelescopeError> {
        send_query::<Self>(Variables {
            now: chrono::Utc::today().naive_utc(),
        })
        .await
    }
}