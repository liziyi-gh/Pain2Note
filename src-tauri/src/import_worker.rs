//! Bounded, persistent worker threads. Only the importing thread writes hands and reports.
use crate::{db, model::Hand, parser};
use rusqlite::{params, Connection};
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

struct Request {
    hero: String,
    raw: String,
}

type ParseResult = Result<Hand, String>;

struct Worker {
    input: Option<Sender<Request>>,
    output: Receiver<ParseResult>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn start(cache: &std::path::Path) -> Result<Self, String> {
        // Open before spawning so initialization errors fail the entire import.
        let conn = db::open(cache)?;
        let (input, requests) = mpsc::channel::<Request>();
        let (responses, output) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("hand-import-worker".into())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    if responses
                        .send(parser::parse(&request.raw, &request.hero, &conn))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("无法启动导入线程：{e}"))?;
        Ok(Self {
            input: Some(input),
            output,
            thread: Some(thread),
        })
    }

    fn send(&self, hero: &str, raw: &str) -> Result<(), String> {
        self.input
            .as_ref()
            .ok_or("导入线程已关闭")?
            .send(Request {
                hero: hero.into(),
                raw: raw.into(),
            })
            .map_err(|_| "导入线程意外退出，本批事务将回滚".into())
    }

    fn receive(&self) -> Result<ParseResult, String> {
        self.output
            .recv()
            .map_err(|_| "导入线程意外退出，本批事务将回滚".into())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Close the queue and join before the temporary cache is removed.
        self.input.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) struct Pool {
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
    pub fn new(capacity: usize) -> Self {
        Self {
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
    ) -> Result<Vec<ParseResult>, String> {
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
            self.workers.push(Worker::start(&path)?);
        }
        for (worker, raw) in self.workers.iter().zip(raw) {
            worker.send(hero, raw)?;
        }
        self.workers
            .iter()
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
        self.workers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_worker_exit_is_reported() {
        let (input, _requests) = mpsc::channel();
        let (responses, output) = mpsc::channel();
        let thread = thread::spawn(move || {
            drop(responses);
            panic!("simulated worker failure");
        });
        let worker = Worker {
            input: Some(input),
            output,
            thread: Some(thread),
        };
        assert!(worker.receive().unwrap_err().contains("导入线程意外退出"));
    }
}
