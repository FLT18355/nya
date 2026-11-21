use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ui;
use crate::util;

#[derive(Clone, Copy)]
enum ArchiveKind {
    Tar,
    Zip,
    SevenZip,
    Rar,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-o" | "--output" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("-o 后面需要输出目录".to_owned());
                };
                output = Some(PathBuf::from(value));
            }
            arg if arg.starts_with('-') => {
                return Err(format!("未知选项: {arg}"));
            }
            arg => {
                if input.is_some() {
                    return Err("一次只能解压一个压缩包".to_owned());
                }
                input = Some(PathBuf::from(arg));
            }
        }
        index += 1;
    }

    let Some(input) = input else {
        print_help();
        return Ok(());
    };
    if !input.is_file() {
        return Err(format!("找不到压缩包: {}", input.display()));
    }

    let kind = detect(&input)?;
    let explicit_output = output.is_some();
    let destination = output.unwrap_or_else(|| default_destination(&input));
    let (created, empty) = prepare_destination(&destination, explicit_output)?;
    if explicit_output && !empty {
        ui::hint("目标目录已有文件，同名文件可能会被覆盖喵。");
    }

    ui::header(ui::ICON_ARCHIVE, "解压喵", "tar / zip / 7z / rar 喵");
    println!(
        "  {}  {}",
        ui::paint("压缩包喵", ui::OVERLAY1),
        ui::paint(util::pretty_path(&input), ui::TEXT)
    );
    println!(
        "  {}  {}",
        ui::paint("输出目录喵", ui::OVERLAY1),
        ui::paint(util::pretty_path(&destination), ui::MAUVE)
    );
    println!();

    let mut command = build_command(kind, &input, &destination)?;
    let result = util::run_with_spinner(&mut command, "解压中");
    if let Err(error) = result {
        if created {
            let _ = fs::remove_dir_all(&destination);
        }
        return Err(error);
    }

    println!();
    ui::success("解压完成喵。");
    println!(
        "  {}  {}",
        ui::paint("位置喵", ui::OVERLAY1),
        ui::paint(util::pretty_path(&destination), ui::MAUVE)
    );
    println!();
    Ok(())
}

fn print_help() {
    println!();
    println!(
        "  {}  {}",
        ui::paint(ui::ICON_ARCHIVE, ui::MAUVE),
        ui::paint_bold("解压喵", ui::TEXT)
    );
    println!();
    println!(
        "  {}",
        ui::paint("用法: nya x <压缩包> [-o 输出目录]", ui::TEXT)
    );
    println!();
    println!("  {}", ui::paint_bold("支持", ui::LAVENDER));
    println!(
        "    {}",
        ui::paint(
            ".tar .tar.gz .tgz .tar.bz2 .tar.xz .tar.zst .zip .7z .rar",
            ui::TEXT
        )
    );
    println!();
    println!(
        "  {}",
        ui::paint(
            "不给 -o 时解压到同名目录；目标目录非空时会停止。",
            ui::OVERLAY1
        )
    );
    println!();
}

fn detect(path: &Path) -> Result<ArchiveKind, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = if name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.bz2")
        || name.ends_with(".tbz2")
        || name.ends_with(".tbz")
        || name.ends_with(".tar.xz")
        || name.ends_with(".txz")
        || name.ends_with(".tar.zst")
        || name.ends_with(".tzst")
        || name.ends_with(".tar")
    {
        ArchiveKind::Tar
    } else if name.ends_with(".zip") {
        ArchiveKind::Zip
    } else if name.ends_with(".7z") {
        ArchiveKind::SevenZip
    } else if name.ends_with(".rar") {
        ArchiveKind::Rar
    } else {
        return Err(format!(
            "不支持这个后缀: {}\n支持 tar 系列、zip、7z、rar",
            path.display()
        ));
    };
    Ok(kind)
}

fn default_destination(archive: &Path) -> PathBuf {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive");
    let lower = name.to_ascii_lowercase();
    let suffixes = [
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tgz", ".tbz2", ".tbz", ".txz", ".tzst",
        ".tar", ".zip", ".7z", ".rar",
    ];
    let stem = suffixes
        .iter()
        .find(|suffix| lower.ends_with(**suffix))
        .map(|suffix| &name[..name.len() - suffix.len()])
        .filter(|stem| !stem.is_empty())
        .unwrap_or("nya-out");
    archive
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(stem)
}

fn prepare_destination(path: &Path, explicit: bool) -> Result<(bool, bool), String> {
    if path.exists() {
        if !path.is_dir() {
            return Err(format!("输出位置不是目录: {}", path.display()));
        }
        let empty = fs::read_dir(path)
            .map_err(|error| format!("读取输出目录失败: {error}"))?
            .next()
            .is_none();
        if !empty && !explicit {
            return Err(format!(
                "同名目录已存在且非空: {}\n用 -o 指定另一个目录。",
                path.display()
            ));
        }
        return Ok((false, empty));
    }
    fs::create_dir_all(path).map_err(|error| format!("创建输出目录失败: {error}"))?;
    Ok((true, true))
}

fn build_command(kind: ArchiveKind, archive: &Path, destination: &Path) -> Result<Command, String> {
    match kind {
        ArchiveKind::Tar => {
            let mut command = Command::new("tar");
            command.arg("-xf").arg(archive).arg("-C").arg(destination);
            Ok(command)
        }
        ArchiveKind::Zip => {
            if util::command_exists("unzip") {
                let mut command = Command::new("unzip");
                command
                    .arg("-q")
                    .arg("-o")
                    .arg(archive)
                    .arg("-d")
                    .arg(destination);
                Ok(command)
            } else {
                seven_zip_command(archive, destination)
            }
        }
        ArchiveKind::SevenZip | ArchiveKind::Rar => seven_zip_command(archive, destination),
    }
}

fn seven_zip_command(archive: &Path, destination: &Path) -> Result<Command, String> {
    let program = if util::command_exists("7zz") {
        "7zz"
    } else if util::command_exists("7z") {
        "7z"
    } else {
        return Err("需要先安装 7z 或 7zz。".to_owned());
    };
    let mut output_arg = OsString::from("-o");
    output_arg.push(destination.as_os_str());
    let mut command = Command::new(program);
    command
        .arg("x")
        .arg("-y")
        .arg("-bd")
        .arg("-bso0")
        .arg("-bsp0")
        .arg(output_arg)
        .arg(archive);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_archive_kinds() {
        assert!(matches!(
            detect(Path::new("a.tar.gz")),
            Ok(ArchiveKind::Tar)
        ));
        assert!(matches!(
            detect(Path::new("a.7z")),
            Ok(ArchiveKind::SevenZip)
        ));
        assert!(detect(Path::new("a.xyz")).is_err());
    }

    #[test]
    fn default_destination_strips_suffixes() {
        assert_eq!(
            default_destination(Path::new("/tmp/kitty.tar.gz")),
            PathBuf::from("/tmp/kitty")
        );
        assert_eq!(
            default_destination(Path::new("/tmp/kitty.zip")),
            PathBuf::from("/tmp/kitty")
        );
    }
}
