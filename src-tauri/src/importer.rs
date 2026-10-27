use crate::{db, model::ImportReport, parser};
use flate2::read::MultiGzDecoder;
use rusqlite::Connection;
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
    sync::LazyLock,
};
use walkdir::WalkDir;

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;
static HAND_START: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)^Poker Hand #").unwrap());

fn supported(name: &str) -> bool {
    [".txt", ".hh", ".log", ".zip", ".gz", ".tgz", ".tar"]
        .iter()
        .any(|ext| name.to_lowercase().ends_with(ext))
}

struct Importer<'a, F: Fn(&ImportReport)> {
    conn: &'a Connection,
    hero: &'a str,
    report: ImportReport,
    bytes: u64,
    entries: usize,
    progress: F,
}

impl<F: Fn(&ImportReport)> Importer<'_, F> {
    fn read(&mut self, reader: impl Read) -> Result<Vec<u8>, String> {
        let allowance = MAX_FILE.min(MAX_TOTAL.saturating_sub(self.bytes));
        let mut data = Vec::new();
        reader
            .take(allowance + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        self.bytes += data.len() as u64;
        if data.len() as u64 > allowance {
            return Err("超过导入大小限制：单文件 64 MiB / 每批展开数据 512 MiB".into());
        }
        Ok(data)
    }

    fn process(&mut self, name: &str, data: Vec<u8>, depth: usize) -> Result<(), String> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES || depth > 4 {
            return Err("超过 10,000 个条目或 4 层压缩嵌套限制".into());
        }
        let lower = name.to_lowercase();
        if lower.ends_with(".zip") {
            let mut archive = zip::ZipArchive::new(Cursor::new(data))
                .map_err(|e| format!("ZIP 无法读取：{e}"))?;
            if archive.len() > MAX_ENTRIES.saturating_sub(self.entries) {
                return Err("ZIP 条目过多".into());
            }
            for i in 0..archive.len() {
                let file = archive
                    .by_index(i)
                    .map_err(|e| format!("ZIP 条目无法读取（可能已加密）：{e}"))?;
                if file.is_dir() {
                    continue;
                }
                if file.enclosed_name().is_none() {
                    self.report.reject(format!("{name}: 不安全的压缩包路径"));
                    continue;
                }
                let child = file.name().to_string();
                if child.starts_with("__MACOSX/")
                    || child.split('/').any(|p| p.starts_with('.'))
                    || !supported(&child)
                {
                    continue;
                }
                let label = format!("{name} → {child}");
                let result = self
                    .read(file)
                    .and_then(|data| self.process(&child, data, depth + 1));
                if let Err(e) = result {
                    self.report.reject(format!("{label}: {e}"));
                }
                if self.bytes > MAX_TOTAL || self.entries > MAX_ENTRIES {
                    break;
                }
            }
        } else if lower.ends_with(".gz") || lower.ends_with(".tgz") {
            let child = if lower.ends_with(".tgz") {
                format!("{}.tar", &name[..name.len() - 4])
            } else {
                name[..name.len() - 3].to_string()
            };
            let expanded = self.read(MultiGzDecoder::new(Cursor::new(data)))?;
            self.process(&child, expanded, depth + 1)?;
        } else if lower.ends_with(".tar") {
            let mut archive = tar::Archive::new(Cursor::new(data));
            for entry in archive.entries().map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if !entry.header().entry_type().is_file() {
                    continue;
                }
                let child = entry
                    .path()
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .to_string();
                if !supported(&child) {
                    continue;
                }
                let result = self
                    .read(entry)
                    .and_then(|data| self.process(&child, data, depth + 1));
                if let Err(e) = result {
                    self.report.reject(format!("{name} → {child}: {e}"));
                }
                if self.bytes > MAX_TOTAL || self.entries > MAX_ENTRIES {
                    break;
                }
            }
        } else {
            self.report.files += 1;
            let text = decode(&data)?.replace("\r\n", "\n").replace('\r', "\n");
            let text = text.trim_start_matches('\u{feff}');
            let starts: Vec<usize> = HAND_START.find_iter(text).map(|m| m.start()).collect();
            if starts.is_empty() {
                return Err("未找到 GG Poker 手牌头".into());
            }
            for (i, &start) in starts.iter().enumerate() {
                let raw = &text[start..*starts.get(i + 1).unwrap_or(&text.len())];
                let id = raw
                    .strip_prefix("Poker Hand #")
                    .and_then(|s| s.split_once(':'))
                    .map(|s| s.0);
                if let Some(id) = id {
                    if db::exists(self.conn, id)? {
                        self.report.duplicates += 1;
                        continue;
                    }
                }
                match parser::parse(raw, self.hero, self.conn) {
                    Ok(hand) => {
                        if db::insert(self.conn, &hand)? {
                            self.report.imported += 1;
                        } else {
                            self.report.duplicates += 1;
                        }
                    }
                    Err(e) => self
                        .report
                        .reject(format!("{name} / {}: {e}", id.unwrap_or("未知手牌"))),
                }
                if i % 50 == 0 {
                    (self.progress)(&self.report);
                }
            }
            (self.progress)(&self.report);
        }
        Ok(())
    }

    fn file(&mut self, path: &Path) {
        let name = path.to_string_lossy();
        if !supported(&name) {
            self.report
                .reject(format!("{name}: 不支持的格式；支持 TXT、ZIP、GZ、TAR、TGZ"));
            return;
        }
        let result = File::open(path)
            .map_err(|e| e.to_string())
            .and_then(|file| self.read(file))
            .and_then(|data| self.process(&name, data, 0));
        if let Err(e) = result {
            self.report.reject(format!("{name}: {e}"));
        }
        (self.progress)(&self.report);
    }
}

fn decode(data: &[u8]) -> Result<String, String> {
    if data.starts_with(&[0xff, 0xfe]) || data.starts_with(&[0xfe, 0xff]) {
        if !data.len().is_multiple_of(2) {
            return Err("损坏的 UTF-16 文本".into());
        }
        let le = data[0] == 0xff;
        let units: Vec<u16> = data[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|_| "无效 UTF-16 文本".into())
    } else {
        String::from_utf8(data.to_vec())
            .map_err(|_| "请导出 UTF-8 或带 BOM 的 UTF-16 英文牌谱".into())
    }
}

pub fn import_paths(
    conn: &mut Connection,
    paths: &[String],
    hero: &str,
    progress: impl Fn(&ImportReport),
) -> Result<ImportReport, String> {
    if hero.trim().is_empty() {
        return Err("Hero 名称不能为空".into());
    }
    if paths.is_empty() {
        return Err("请选择要导入的文件或文件夹".into());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut importer = Importer {
        conn: &tx,
        hero: hero.trim(),
        report: ImportReport::default(),
        bytes: 0,
        entries: 0,
        progress,
    };
    for name in paths {
        let path = Path::new(name);
        if path.is_symlink() {
            importer.report.reject(format!("{name}: 跳过符号链接"));
            continue;
        }
        if path.is_dir() {
            for entry in WalkDir::new(path).follow_links(false).sort_by_file_name() {
                match entry {
                    Ok(entry)
                        if entry.file_type().is_file()
                            && supported(&entry.path().to_string_lossy()) =>
                    {
                        importer.file(entry.path())
                    }
                    Err(e) => importer.report.reject(e.to_string()),
                    _ => (),
                }
                if importer.bytes > MAX_TOTAL || importer.entries > MAX_ENTRIES {
                    break;
                }
            }
        } else {
            importer.file(path);
        }
        if importer.bytes > MAX_TOTAL || importer.entries > MAX_ENTRIES {
            break;
        }
    }
    if importer.report.files == 0 && importer.report.rejected == 0 {
        importer
            .report
            .reject("所选路径没有可导入的牌谱文件".into());
    }
    let report = importer.report;
    db::save_report(&tx, &report)?;
    tx.commit()
        .map_err(|e| format!("导入未保存，事务已回滚：{e}"))?;
    Ok(report)
}
