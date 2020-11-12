use crate::{
    models::Confirmation,
    web::Template
};
use crate::templates::forms::common::password::PasswordField;

/// The template for new account confirmations.
/// The user is prompted to input a name and password to seed their account.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewUserConfirmation {
    /// The confirmation that spawned this form.
    invite: Confirmation,
    /// The name previously entered into this form if there was one.
    name: Option<String>,
    /// The user's new password.
    password: PasswordField,
    /// The password again. Should match the other password field.
    confirm_password: PasswordField,
}

impl Template for NewUserConfirmation {
    const TEMPLATE_NAME: &'static str = "forms/confirm/new_user";
}

/// An email confirmed for an existing user.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExistingUserConfirmation {
    /// The invite that spawned this page.
    invite: Confirmation,
    /// An error message if an error occurred.
    error_message: Option<String>,
}

impl Template for ExistingUserConfirmation {
    const TEMPLATE_NAME: &'static str = "forms/confirm/existing_user";
}