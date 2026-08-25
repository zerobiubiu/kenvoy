//! SMTP 邮件发送（[kenvoy.toml] `[mail]`）。
//!
//! 设计原则：邮件通道**失败不中断业务流程**。
//! - 未配置 `[mail]` 或配置不完整：发件直接跳过，仅 eprintln；
//! - 配置完整但发送失败（网络/认证错误）：eprintln 记录原因，调用方继续返回 200。

use std::fmt;

use crate::config::MailConfig;

/// 邮件错误（不向调用方抛出，仅内部日志）。
#[derive(Debug)]
pub struct MailError(String);

impl fmt::Display for MailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MailError {}

/// 发送密码重置邮件。
///
/// `to` 收件人邮箱；`username` 用于在邮件中称呼用户；`token` 密码重置明文 token。
///
/// **设计原则：邮件通道失败不中断业务流程**——未配置 / 配置不完整 / SMTP 错误
/// 均 eprintln 后返回 `Ok(())`，不向调用方返回错误。当前为同步阻塞 SMTP
/// （lettre 0.11 同步传输），调用方应包在 `tokio::task::spawn_blocking` 中。
///
/// # Errors
/// 当前实现按设计不返回错误，MailError 保留供未来异步传输或单元测试。
#[cfg(feature = "mail")]
pub fn send_password_reset(
    cfg: &MailConfig,
    to: &str,
    username: &str,
    token: &str,
) -> Result<(), MailError> {
    use lettre::message::{Mailbox, Message, header::ContentType};
    use lettre::{SmtpTransport, Transport};

    if !cfg.is_enabled() {
        eprintln!(
            "[kenvoy/mail] [mail] 段未配置完整（smtp_host/user/pass/from 至少任一缺失）；\
             跳过向 {to} 发送密码重置邮件，token 已通过 eprintln 与响应体（dev mode）输出。"
        );
        return Ok(());
    }
    let host = cfg.smtp_host.as_deref().unwrap_or("");
    let port = cfg.smtp_port.unwrap_or(587);
    let user = cfg.smtp_user.as_deref().unwrap_or("");
    let pass = cfg.smtp_pass.as_deref().unwrap_or("");
    let from = cfg.from.as_deref().unwrap_or("");
    let base = cfg.reset_url_base.as_deref().unwrap_or("");
    let reset_link = format!(
        "{}/__kenvoy/auth/reset?token={token}",
        base.trim_end_matches('/')
    );

    let from_mb: Mailbox = match from.parse() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[kenvoy/mail] 发件人邮箱格式无效 `{from}`: {e}");
            return Ok(());
        }
    };
    let to_mb: Mailbox = match to.parse() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[kenvoy/mail] 收件人邮箱格式无效 `{to}`: {e}");
            return Ok(());
        }
    };
    let subject = "Kenvoy 密码重置".to_string();
    let body = format!(
        "您好 {username}，\n\n\
         我们收到了您的密码重置请求。请点击以下链接重置密码（1 小时内有效）：\n\n\
         {reset_link}\n\n\
         如果不是您本人操作，请忽略本邮件。\n\n\
         — Kenvoy"
    );
    let email = match Message::builder()
        .from(from_mb)
        .to(to_mb)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body)
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[kenvoy/mail] 构造邮件失败：{e}");
            return Ok(());
        }
    };

    // 0.11 同步 SMTP 客户端：用 block_in_place 避免阻塞异步运行时
    let builder = match SmtpTransport::starttls_relay(host) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[kenvoy/mail] SMTP 主机配置无效 host={host} err={e}");
            return Ok(());
        }
    };
    let transport = builder.port(port).credentials((user, pass).into()).build();
    let send_res = tokio::task::block_in_place(|| transport.send(&email));
    if let Err(e) = send_res {
        eprintln!("[kenvoy/mail] 密码重置邮件发送失败 to={to} host={host}:{port} err={e}");
        return Ok(()); // 不向调用方返回错误
    }
    eprintln!("[kenvoy/mail] 密码重置邮件已发送 to={to}");
    Ok(())
}

/// 未启用 mail feature 时的占位实现：仅 eprintln 后返回 `Ok(())`。
///
/// # Errors
/// 当前实现按设计不返回错误。
#[cfg(not(feature = "mail"))]
pub fn send_password_reset(
    _cfg: &MailConfig,
    to: &str,
    _username: &str,
    _token: &str,
) -> Result<(), MailError> {
    eprintln!("[kenvoy/mail] 未启用 mail feature；跳过向 {to} 发送密码重置邮件。");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unconfigured_mail_silently_skips() {
        let cfg = MailConfig::default();
        assert!(send_password_reset(&cfg, "user@example.com", "u", "t").is_ok());
    }
}
