//! 固定根目录布局的 ZIP 与相邻 SHA256 文件；失败不保留半成品。

use super::options::Options;
use crate::Result;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
};
use zip::{CompressionMethod, DateTime, ZipWriter, write::SimpleFileOptions};

pub(super) fn create(options: &Options) -> Result<()> {
    fs::create_dir_all(&options.output)?;
    let name = &options.archive_name;
    let archive = options.output.join(name);
    let checksum = options.output.join(format!("{name}.sha256"));
    if checksum.try_exists()? {
        return Err(format!("拒绝覆盖已有校验文件：{}", checksum.display()).into());
    }
    let file = File::create_new(&archive)
        .map_err(|error| format!("无法独占创建归档 {}：{error}", archive.display()))?;
    let mut pending = Pending {
        archive: &archive,
        checksum: None,
        complete: false,
    };
    write_archive(file, options)?;
    let digest = sha256(&archive)?;
    let mut file = File::create_new(&checksum)?;
    pending.checksum = Some(&checksum);
    writeln!(file, "{digest}  {name}")?;
    file.sync_all()?;
    pending.complete = true;
    println!("ZIP：{}", archive.display());
    println!("SHA256：{digest}；{}", checksum.display());
    Ok(())
}

fn write_archive(file: File, options: &Options) -> Result<()> {
    let mut archive = ZipWriter::new(file);
    for (name, path, permissions) in [
        (
            options.target.binary_name(),
            options.binary.as_path(),
            0o755,
        ),
        ("README.md", options.readme.as_path(), 0o644),
        ("LICENSE", options.license.as_path(), 0o644),
    ] {
        let mut source = File::open(path)?;
        let metadata = source.metadata()?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!("归档输入不是非空普通文件：{}", path.display()).into());
        }
        let entry = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(DateTime::default())
            .unix_permissions(permissions)
            .large_file(metadata.len() >= (1_u64 << 32));
        archive.start_file(name, entry)?;
        io::copy(&mut source, &mut archive)?;
    }
    archive.finish()?.sync_all()?;
    Ok(())
}

fn sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = hash.finalize();
    let mut text = String::with_capacity(2 * digest.len());
    for byte in digest {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(text)
}

struct Pending<'a> {
    archive: &'a Path,
    checksum: Option<&'a Path>,
    complete: bool,
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        for path in std::iter::once(self.archive).chain(self.checksum.as_deref()) {
            if let Err(error) = fs::remove_file(path)
                && error.kind() != io::ErrorKind::NotFound
            {
                eprintln!("无法清理本次分发半成品 {}：{error}", path.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::temporary::TemporaryDirectory;
    use super::{Options, create, sha256};
    use crate::Result;
    use std::fs::{self, File};

    #[test]
    fn selected_archive_name_drives_sidecar_without_overwriting() -> Result<()> {
        let temporary = TemporaryDirectory::create()?;
        let root = temporary.path();
        let options = Options::parse(
            root,
            &[
                "--binary".into(),
                root.join("kenvoy").into_os_string(),
                "--target".into(),
                "x86_64-unknown-linux-musl".into(),
                "--version".into(),
                "1.2.3".into(),
                "--output".into(),
                root.join("artifacts").into_os_string(),
                "--archive-name".into(),
                "selected-name.zip".into(),
            ],
        )?;
        fs::create_dir_all(root.join("crates/kenvoy"))?;
        fs::write(&options.binary, b"binary")?;
        fs::write(&options.readme, b"readme")?;
        fs::write(&options.license, b"license")?;
        create(&options)?;
        let archive = options.output.join("selected-name.zip");
        let checksum = options.output.join("selected-name.zip.sha256");
        let contents = fs::read(&archive)?;
        let expected = format!("{}  selected-name.zip\n", sha256(&archive)?);
        assert_eq!(fs::read_to_string(&checksum)?, expected);
        let zip = zip::ZipArchive::new(File::open(&archive)?)?;
        let mut names = zip.file_names().collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, ["LICENSE", "README.md", "kenvoy"]);
        drop(zip);
        assert!(create(&options).is_err());
        assert_eq!(fs::read(&archive)?, contents);
        assert_eq!(fs::read_to_string(&checksum)?, expected);
        fs::remove_file(&checksum)?;
        assert!(create(&options).is_err());
        assert_eq!(fs::read(&archive)?, contents);
        assert!(!checksum.exists());
        Ok(())
    }
}
