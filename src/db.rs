use crate::analysis::{analyze, AnalysisRequest, AnalysisResponse, Dataset};
use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub id: String,
    pub dataset_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub included_point_ids: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRun {
    pub id: i64,
    pub fingerprint: String,
    pub branch_id: Option<String>,
    pub request: AnalysisRequest,
    pub response: AnalysisResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub snapshot_version: String,
    pub datasets: Vec<Dataset>,
    pub branches: Vec<Branch>,
    pub runs: Vec<StoredRun>,
}

pub struct AppState {
    pub db: Mutex<Connection>,
}

impl AppState {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let connection = Connection::open(path)?;
        let state = Self {
            db: Mutex::new(connection),
        };
        state.init()?;
        Ok(state)
    }

    pub fn in_memory() -> anyhow::Result<Self> {
        let connection = Connection::open_in_memory()?;
        let state = Self {
            db: Mutex::new(connection),
        };
        state.init()?;
        Ok(state)
    }

    fn init(&self) -> anyhow::Result<()> {
        let db = self.db.lock().expect("database lock");
        db.pragma_update(None, "foreign_keys", "ON")?;
        let has_native = db
            .prepare("PRAGMA table_info(datasets)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|column| column == "native_convention");
        if !has_native {
            let _ = db.execute("ALTER TABLE datasets ADD COLUMN native_convention TEXT", []);
        }
        db.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS datasets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                common_lead_default TEXT NOT NULL DEFAULT '',
                native_convention TEXT,
                payload TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS branches (
                id TEXT PRIMARY KEY,
                dataset_id TEXT NOT NULL REFERENCES datasets(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                parent_id TEXT REFERENCES branches(id) ON DELETE SET NULL,
                included_point_ids TEXT NOT NULL,
                note TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fingerprint TEXT NOT NULL UNIQUE,
                branch_id TEXT REFERENCES branches(id) ON DELETE SET NULL,
                request_json TEXT NOT NULL,
                response_json TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn seed_if_empty(&self, fixture_json: &str) -> anyhow::Result<()> {
        let datasets: Vec<Dataset> = serde_json::from_str(fixture_json)?;
        let db = self.db.lock().expect("database lock");
        let count: i64 = db.query_row("SELECT COUNT(*) FROM datasets", [], |row| row.get(0))?;
        if count == 0 {
            for dataset in &datasets {
                upsert_dataset_locked(&db, dataset)?;
            }
        }
        Ok(())
    }

    pub fn list_datasets(&self) -> anyhow::Result<Vec<Dataset>> {
        let db = self.db.lock().expect("database lock");
        let mut statement = db.prepare("SELECT payload FROM datasets ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            let payload: String = row.get(0)?;
            Ok(payload)
        })?;
        let mut datasets = Vec::new();
        for row in rows {
            datasets.push(serde_json::from_str(&row?)?);
        }
        Ok(datasets)
    }

    pub fn get_dataset(&self, id: &str) -> anyhow::Result<Option<Dataset>> {
        let db = self.db.lock().expect("database lock");
        let payload = db
            .query_row(
                "SELECT payload FROM datasets WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(match payload {
            Some(value) => Some(serde_json::from_str(&value)?),
            None => None,
        })
    }

    pub fn upsert_dataset(&self, dataset: &Dataset) -> anyhow::Result<()> {
        let db = self.db.lock().expect("database lock");
        upsert_dataset_locked(&db, dataset)
    }

    pub fn create_branch(&self, mut branch: Branch) -> anyhow::Result<Branch> {
        if branch.id.is_empty() {
            let included = serde_json::to_string(&branch.included_point_ids)?;
            let mut hasher = Sha256::new();
            hasher.update(branch.dataset_id.as_bytes());
            hasher.update(b"\n");
            hasher.update(included.as_bytes());
            hasher.update(b"\n");
            hasher.update(branch.name.as_bytes());
            let digest = hasher.finalize();
            branch.id = format!("br-{:.16x}", digest);
        }
        let db = self.db.lock().expect("database lock");
        let included = serde_json::to_string(&branch.included_point_ids)?;
        db.execute(
            "INSERT OR REPLACE INTO branches(id, dataset_id, name, parent_id, included_point_ids, note)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                branch.id,
                branch.dataset_id,
                branch.name,
                branch.parent_id,
                included,
                branch.note
            ],
        )?;
        Ok(branch)
    }

    pub fn list_branches(&self, dataset_id: &str) -> anyhow::Result<Vec<Branch>> {
        let db = self.db.lock().expect("database lock");
        let mut statement = db.prepare(
            "SELECT id, dataset_id, name, parent_id, included_point_ids, note
             FROM branches WHERE dataset_id = ?1 ORDER BY name, id",
        )?;
        let rows = statement.query_map(params![dataset_id], branch_from_row)?;
        let mut branches = Vec::new();
        for row in rows {
            branches.push(row?);
        }
        Ok(branches)
    }

    pub fn save_run(
        &self,
        request: &AnalysisRequest,
        response: &AnalysisResponse,
        branch_id: Option<String>,
    ) -> anyhow::Result<i64> {
        let db = self.db.lock().expect("database lock");
        db.execute(
            "INSERT OR IGNORE INTO runs(fingerprint, branch_id, request_json, response_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                response.run_fingerprint,
                branch_id,
                serde_json::to_string(request)?,
                serde_json::to_string(response)?
            ],
        )?;
        db.query_row(
            "SELECT id FROM runs WHERE fingerprint = ?1",
            params![response.run_fingerprint],
            |row| row.get(0),
        )
        .context("run id missing after save")
    }

    pub fn list_runs(&self) -> anyhow::Result<Vec<StoredRun>> {
        let db = self.db.lock().expect("database lock");
        let mut statement = db.prepare(
            "SELECT id, fingerprint, branch_id, request_json, response_json FROM runs ORDER BY id",
        )?;
        let rows = statement.query_map([], run_from_row)?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(row?);
        }
        Ok(runs)
    }

    pub fn export_snapshot(&self) -> anyhow::Result<Snapshot> {
        let db = self.db.lock().expect("database lock");
        let datasets: Vec<Dataset> = {
            let mut statement = db.prepare("SELECT payload FROM datasets ORDER BY id")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            let mut values = Vec::new();
            for payload in rows {
                values.push(serde_json::from_str(&payload?)?);
            }
            values
        };
        let branches = {
            let mut statement = db.prepare(
                "SELECT id, dataset_id, name, parent_id, included_point_ids, note FROM branches ORDER BY id",
            )?;
            let rows = statement.query_map([], branch_from_row)?;
            let mut values = Vec::new();
            for row in rows {
                values.push(row?);
            }
            values
        };
        let runs = {
            let mut statement = db.prepare(
                "SELECT id, fingerprint, branch_id, request_json, response_json FROM runs ORDER BY id",
            )?;
            let rows = statement.query_map([], run_from_row)?;
            let mut values = Vec::new();
            for row in rows {
                values.push(row?);
            }
            values
        };
        Ok(Snapshot {
            snapshot_version: "concordia-jiaotai-snapshot-v1".into(),
            datasets,
            branches,
            runs,
        })
    }

    pub fn clear_all(&self) -> anyhow::Result<()> {
        let db = self.db.lock().expect("database lock");
        db.execute_batch("DELETE FROM runs; DELETE FROM branches; DELETE FROM datasets;")?;
        Ok(())
    }

    pub fn import_snapshot(&self, snapshot: &Snapshot) -> anyhow::Result<ImportReport> {
        if snapshot.snapshot_version != "concordia-jiaotai-snapshot-v1" {
            anyhow::bail!("不支持的快照版本");
        }
        let mut verified = 0usize;
        {
            let mut db = self.db.lock().expect("database lock");
            let transaction = db.transaction()?;
            transaction
                .execute_batch("DELETE FROM runs; DELETE FROM branches; DELETE FROM datasets;")?;
            for dataset in &snapshot.datasets {
                upsert_dataset_locked(&transaction, dataset)?;
            }
            for branch in &snapshot.branches {
                let included = serde_json::to_string(&branch.included_point_ids)?;
                transaction.execute(
                    "INSERT INTO branches(id, dataset_id, name, parent_id, included_point_ids, note)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        branch.id,
                        branch.dataset_id,
                        branch.name,
                        branch.parent_id,
                        included,
                        branch.note
                    ],
                )?;
            }
            for run in &snapshot.runs {
                let dataset = snapshot
                    .datasets
                    .iter()
                    .find(|item| item.id == run.request.dataset_id)
                    .ok_or_else(|| anyhow::anyhow!("运行引用了缺失数据集"))?;
                let recomputed = analyze(run.request.clone(), dataset)?;
                if recomputed.run_fingerprint != run.fingerprint
                    || recomputed.run_fingerprint != run.response.run_fingerprint
                {
                    anyhow::bail!("运行 {} 的指纹复核失败", run.id);
                }
                transaction.execute(
                    "INSERT INTO runs(id, fingerprint, branch_id, request_json, response_json)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        run.id,
                        run.fingerprint,
                        run.branch_id,
                        serde_json::to_string(&run.request)?,
                        serde_json::to_string(&run.response)?
                    ],
                )?;
                verified += 1;
            }
            transaction.commit()?;
        }
        Ok(ImportReport {
            datasets: snapshot.datasets.len(),
            branches: snapshot.branches.len(),
            runs_verified: verified,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub datasets: usize,
    pub branches: usize,
    pub runs_verified: usize,
}

fn upsert_dataset_locked(db: &Connection, dataset: &Dataset) -> anyhow::Result<()> {
    db.execute(
        "INSERT INTO datasets(id, name, description, common_lead_default, payload)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            name=excluded.name,
            description=excluded.description,
            common_lead_default=excluded.common_lead_default,
            payload=excluded.payload",
        params![
            dataset.id,
            dataset.name,
            dataset.description,
            dataset.common_lead_default,
            serde_json::to_string(dataset)?
        ],
    )?;
    Ok(())
}

fn branch_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Branch> {
    let included_json: String = row.get(4)?;
    Ok(Branch {
        id: row.get(0)?,
        dataset_id: row.get(1)?,
        name: row.get(2)?,
        parent_id: row.get(3)?,
        included_point_ids: serde_json::from_str(&included_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        note: row.get(5)?,
    })
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRun> {
    let request_json: String = row.get(3)?;
    let response_json: String = row.get(4)?;
    Ok(StoredRun {
        id: row.get(0)?,
        fingerprint: row.get(1)?,
        branch_id: row.get(2)?,
        request: serde_json::from_str(&request_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        response: serde_json::from_str(&response_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}
