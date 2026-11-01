//! 与传输层无关的字段约束；DSL 默认值、资源写入和浏览器规则共享同一契约。

mod errors;
mod field;
mod files;
mod record;

pub use errors::{ValidationError, ValidationErrors};
pub use field::{MAX_SAFE_INTEGER, validate_default, validate_field};
pub use files::{
    mime_matches_accept, valid_image_src, valid_mime_type, valid_upload_id,
    validate_file_constraints, validate_uploaded_file,
};
pub use record::{WriteMode, parse_record_json, validate_record};

#[cfg(test)]
mod tests;
