use crate::domain::{
    authorization::{self, AuthorizationError},
    model::{ApplicationIR, Role},
};

#[derive(Clone, Copy)]
pub enum PageAccess {
    Index,
    Login,
}

pub fn page(
    application: &ApplicationIR,
    path: &str,
    role: Role,
    has_assets: bool,
) -> Result<PageAccess, AuthorizationError> {
    if path == "/admin" || path == "/admin/login" || path.starts_with("/admin/") {
        if path != "/admin/login" {
            if path != "/admin"
                && !application
                    .resources
                    .iter()
                    .any(|resource| path == format!("/admin/{}", resource.name))
            {
                return Err(AuthorizationError::UnknownResource);
            }
            if role == Role::Anonymous && has_assets {
                return Ok(PageAccess::Login);
            }
            if role != Role::Admin {
                return Err(AuthorizationError::Denied(role));
            }
        }
    } else if let Some(page) = application.pages.iter().find(|page| page.path == path) {
        if !authorization::permits(role, page.permission) {
            return Err(AuthorizationError::Denied(role));
        }
    } else {
        return Err(AuthorizationError::UnknownResource);
    }
    Ok(PageAccess::Index)
}
