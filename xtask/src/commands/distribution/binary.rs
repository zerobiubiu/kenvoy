//! 直接读取 PE、ELF、Mach-O 头；不依赖文件名推断架构。

use super::target::{Platform, Target};
use crate::Result;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(super) fn verify(path: &Path, target: Target) -> Result<()> {
    let mut file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(format!("分发二进制不是普通文件：{}", path.display()).into());
    }
    match target.platform {
        Platform::Windows => verify_pe(&mut file, target.arm64)?,
        Platform::Linux => verify_elf(&mut file, target.arm64)?,
        Platform::Macos => verify_macho(&mut file, target.arm64)?,
    }
    println!("可执行格式与架构匹配：{}", target.triple);
    Ok(())
}

fn verify_pe(file: &mut File, arm64: bool) -> Result<()> {
    let dos = read_at::<64>(file, 0)?;
    if &dos[..2] != b"MZ" {
        return Err("Windows 产物缺少 DOS/PE 文件头".into());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into()?) as u64;
    let pe = read_at::<26>(file, offset)?;
    let machine = u16::from_le_bytes(pe[4..6].try_into()?);
    let characteristics = u16::from_le_bytes(pe[22..24].try_into()?);
    let optional_size = u16::from_le_bytes(pe[20..22].try_into()?);
    if &pe[..4] != b"PE\0\0"
        || machine != if arm64 { 0xaa64 } else { 0x8664 }
        || characteristics & 0x0002 == 0
        || characteristics & 0x2000 != 0
        || optional_size < 112
        || u16::from_le_bytes(pe[24..26].try_into()?) != 0x020b
    {
        return Err("Windows 产物不是目标架构的 PE32+ 可执行文件".into());
    }
    read_at::<112>(file, offset + 24)?;
    Ok(())
}

fn verify_elf(file: &mut File, arm64: bool) -> Result<()> {
    let header = read_at::<64>(file, 0)?;
    let kind = u16::from_le_bytes(header[16..18].try_into()?);
    let machine = u16::from_le_bytes(header[18..20].try_into()?);
    let entry = u64::from_le_bytes(header[24..32].try_into()?);
    if &header[..4] != b"\x7fELF"
        || header[4..7] != [2, 1, 1]
        || !matches!(kind, 2 | 3)
        || machine != if arm64 { 183 } else { 62 }
        || entry == 0
        || u16::from_le_bytes(header[52..54].try_into()?) != 64
    {
        return Err("Linux 产物不是目标架构的小端 ELF64 可执行文件".into());
    }
    let offset = u64::from_le_bytes(header[32..40].try_into()?);
    let size = u16::from_le_bytes(header[54..56].try_into()?) as u64;
    let count = u16::from_le_bytes(header[56..58].try_into()?) as u64;
    let end = offset
        .checked_add(size * count)
        .ok_or("ELF 程序头偏移溢出")?;
    if size != 56 || count == 0 || end > file.metadata()?.len() {
        return Err("ELF 程序头表无效或被截断".into());
    }
    let mut loadable = false;
    for index in 0..count {
        let program = read_at::<56>(file, offset + size * index)?;
        match u32::from_le_bytes(program[..4].try_into()?) {
            1 => loadable = true,
            3 => return Err("musl 分发产物包含动态解释器，必须构建静态可执行文件".into()),
            _ => {}
        }
    }
    if !loadable {
        return Err("ELF 产物没有可加载程序段".into());
    }
    Ok(())
}

fn verify_macho(file: &mut File, arm64: bool) -> Result<()> {
    let header = read_at::<32>(file, 0)?;
    let cpu = u32::from_le_bytes(header[4..8].try_into()?);
    let kind = u32::from_le_bytes(header[12..16].try_into()?);
    let commands = u32::from_le_bytes(header[16..20].try_into()?);
    let command_size = u32::from_le_bytes(header[20..24].try_into()?) as u64;
    if header[..4] != [0xcf, 0xfa, 0xed, 0xfe]
        || cpu != if arm64 { 0x0100_000c } else { 0x0100_0007 }
        || kind != 2
        || commands == 0
        || command_size < u64::from(commands) * 8
        || 32 + command_size > file.metadata()?.len()
    {
        return Err("macOS 产物不是目标架构的单架构 Mach-O 64 可执行文件".into());
    }
    Ok(())
}

fn read_at<const SIZE: usize>(file: &mut File, offset: u64) -> Result<[u8; SIZE]> {
    let mut bytes = [0; SIZE];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut bytes)
        .map_err(|error| format!("可执行文件头无效或被截断（偏移 {offset}）：{error}"))?;
    Ok(bytes)
}
