//! Bounded, persistent OS processes. Only the parent writes hands and reports.
use crate::{db, model::Hand, parser};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

const FLAG: &str = "--pain2note-import-worker";

#[derive(Serialize, Deserialize)]
struct Request {
    hero: String,
    raw: String,
}

#[derive(Serialize, Deserialize)]
struct Response {
    pid: u32,
    result: Result<Hand, String>,
}

/// Called before initializing Tauri, so workers never create windows.
pub fn run_if_requested() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(FLAG)) {
        return;
    }
    let result = args
        .next()
        .ok_or_else(|| "缺少 worker 权益缓存路径".to_string())
        .and_then(|path| serve(Path::new(&path)));
    if let Err(error) = result {
        eprintln!("导入子进程失败：{error}");
        std::process::exit(1);
    }
    std::process::exit(0);
}

fn serve(path: &Path) -> Result<(), String> {
    let conn = db::open(path)?;
    let input = std::io::stdin();
    let mut output = BufWriter::new(std::io::stdout().lock());
    for line in input.lock().lines() {
        let request: Request =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let response = Response {
            pid: std::process::id(),
            result: parser::parse(&request.raw, &request.hero, &conn),
        };
        serde_json::to_writer(&mut output, &response).map_err(|e| e.to_string())?;
        output.write_all(b"\n").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

struct Worker {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Worker {
    fn start(executable: &Path, cache: &Path) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .arg(FLAG)
            .arg(cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("无法启动导入子进程：{e}"))?;
        let input = BufWriter::new(child.stdin.take().ok_or("无法连接 worker stdin")?);
        let output = BufReader::new(child.stdout.take().ok_or("无法连接 worker stdout")?);
        Ok(Self {
            child,
            input,
            output,
        })
    }

    fn send(&mut self, hero: &str, raw: &str) -> Result<(), String> {
        serde_json::to_writer(
            &mut self.input,
            &Request {
                hero: hero.into(),
                raw: raw.into(),
            },
        )
        .map_err(|e| format!("无法发送 worker 任务：{e}"))?;
        self.input
            .write_all(b"\n")
            .and_then(|()| self.input.flush())
            .map_err(|e| format!("无法发送 worker 任务：{e}"))
    }

    fn receive(&mut self) -> Result<Result<Hand, String>, String> {
        let mut line = String::new();
        if self
            .output
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            return Err("导入子进程意外退出，本批事务将回滚".into());
        }
        let response: Response =
            serde_json::from_str(&line).map_err(|e| format!("无效 worker 响应：{e}"))?;
        if response.pid != self.child.id() {
            return Err("worker 进程身份不匹配".into());
        }
        Ok(response.result)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Also reap children on errors and transaction rollback.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) struct Pool {
    executable: PathBuf,
    pub capacity: usize,
    workers: Vec<Worker>,
    // Drop the connection before deleting the directory (also works on Windows).
    cache: Option<(Connection, tempfile::TempDir)>,
}

fn copy_cache(source: &Connection, target: &Connection) -> Result<(), String> {
    let mut query = source
        .prepare("SELECT key,equity,method,boards FROM equity_cache")
        .map_err(|e| e.to_string())?;
    let mut rows = query.query([]).map_err(|e| e.to_string())?;
    let mut insert = target.prepare_cached("INSERT INTO equity_cache(key,equity,method,boards) VALUES (?1,?2,?3,?4) ON CONFLICT(key) DO NOTHING")
        .map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        insert
            .execute(params![
                row.get::<_, String>(0).map_err(|e| e.to_string())?,
                row.get::<_, f64>(1).map_err(|e| e.to_string())?,
                row.get::<_, String>(2).map_err(|e| e.to_string())?,
                row.get::<_, i64>(3).map_err(|e| e.to_string())?
            ])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl Pool {
    pub fn new(executable: PathBuf, capacity: usize) -> Self {
        Self {
            executable,
            capacity: capacity.clamp(1, 8),
            workers: vec![],
            cache: None,
        }
    }

    pub fn len(&self) -> usize {
        self.workers.len()
    }

    pub fn parse(
        &mut self,
        conn: &Connection,
        hero: &str,
        raw: &[&str],
    ) -> Result<Vec<Result<Hand, String>>, String> {
        if self.cache.is_none() {
            let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
            let mut cache = db::open(&directory.path().join("equity.sqlite3"))?;
            let tx = cache.transaction().map_err(|e| e.to_string())?;
            copy_cache(conn, &tx)?;
            tx.commit().map_err(|e| e.to_string())?;
            self.cache = Some((cache, directory));
        }
        let path = self.cache.as_ref().unwrap().1.path().join("equity.sqlite3");
        while self.workers.len() < raw.len().min(self.capacity) {
            self.workers.push(Worker::start(&self.executable, &path)?);
        }
        for (worker, raw) in self.workers.iter_mut().zip(raw) {
            worker.send(hero, raw)?;
        }
        self.workers
            .iter_mut()
            .take(raw.len())
            .map(Worker::receive)
            .collect()
    }

    pub fn merge_cache(&mut self, conn: &Connection) -> Result<(), String> {
        self.workers.clear();
        if let Some((cache, _)) = &self.cache {
            copy_cache(cache, conn)?;
        }
        Ok(())
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        // Close processes before the temporary cache directory is removed.
        self.workers.clear();
    }
}
