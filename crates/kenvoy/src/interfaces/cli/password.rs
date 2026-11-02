use super::error::{self, CliError as Error, Result};

pub(crate) fn read_password(from_stdin: bool) -> Result<String> {
    use std::io::{BufRead, IsTerminal, Read};
    if from_stdin {
        let mut bytes = Vec::new();
        let mut input = std::io::stdin().lock().take(131);
        input
            .read_until(b'\n', &mut bytes)
            .map_err(|source| error::io("标准输入", source))?;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.len() > 128 {
            return Err(Error::InvalidConfig(
                "密码 UTF-8 长度不能超过 128 字节".into(),
            ));
        }
        return String::from_utf8(bytes)
            .map_err(|_| Error::InvalidConfig("密码必须是有效 UTF-8".into()));
    }
    if !std::io::stdin().is_terminal() {
        return Err(Error::InvalidConfig(
            "非交互输入需明确使用 --password-stdin；不能通过命令行参数传密码".into(),
        ));
    }
    let first = prompt_password("请输入密码（12—128 UTF-8 字节）：")
        .map_err(|source| error::io("终端", source))?;
    let second = prompt_password("请再次输入密码：").map_err(|source| error::io("终端", source))?;
    if first != second {
        return Err(Error::InvalidConfig("两次密码输入不一致".into()));
    }
    Ok(first)
}

fn prompt_password(prompt: &str) -> std::io::Result<String> {
    use std::io::Write;
    {
        // Rust 终端输出使用 Unicode，避免把 UTF-8 字节直接写入 Windows CONOUT$。
        let mut output = std::io::stderr().lock();
        output.write_all(prompt.as_bytes())?;
        output.flush()?;
    }
    rpassword::read_password()
}
