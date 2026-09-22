use crate::analysis::{analyze_input, fixed_runs};
use crate::model::{ExportFile, RunInput, RunResult};
use rusqlite::{params, Connection};
use std::sync::Mutex;

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &str) -> Result<Self, rusqlite::Error> {
        let connection = Connection::open(path)?;
        let database = Self {
            connection: Mutex::new(connection),
        };
        database.init()?;
        Ok(database)
    }

    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let connection = Connection::open_in_memory()?;
        let database = Self {
            connection: Mutex::new(connection),
        };
        database.init()?;
        Ok(database)
    }

    fn init(&self) -> Result<(), rusqlite::Error> {
        let connection = self.connection.lock().expect("database lock");
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                input_json TEXT NOT NULL,
                equation_version TEXT NOT NULL
            );
            "#,
        )
    }

    pub fn seed_fixtures(&self) -> Result<usize, rusqlite::Error> {
        let mut count = 0;
        for run in fixed_runs() {
            if self.upsert_run(run)?.is_some() {
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn upsert_run(&self, input: RunInput) -> Result<Option<String>, rusqlite::Error> {
        let result = analyze_input(input.clone());
        let payload = serde_json::to_string(&input).expect("run input serializes");
        let connection = self.connection.lock().expect("database lock");
        let existing: i64 = connection.query_row(
            "SELECT COUNT(*) FROM runs WHERE id = ?1",
            params![result.id],
            |row| row.get(0),
        )?;
        connection.execute(
            "INSERT OR REPLACE INTO runs (id, name, input_json, equation_version) VALUES (?1, ?2, ?3, ?4)",
            params![result.id, input.name, payload, result.equation_version],
        )?;
        Ok((existing == 0).then_some(result.id))
    }

    pub fn list_runs(&self) -> Result<Vec<RunResult>, rusqlite::Error> {
        let inputs = self.list_inputs()?;
        Ok(inputs.into_iter().map(analyze_input).collect())
    }

    pub fn list_inputs(&self) -> Result<Vec<RunInput>, rusqlite::Error> {
        let connection = self.connection.lock().expect("database lock");
        let mut statement = connection.prepare("SELECT input_json FROM runs ORDER BY name, id")?;
        let rows = statement.query_map([], |row| {
            let payload: String = row.get(0)?;
            Ok(payload)
        })?;
        let mut inputs = Vec::new();
        for row in rows {
            let input: RunInput = serde_json::from_str(&row?).expect("stored JSON is valid");
            inputs.push(input);
        }
        Ok(inputs)
    }

    pub fn get_run(&self, id: &str) -> Result<Option<RunResult>, rusqlite::Error> {
        let input = self.get_input(id)?;
        Ok(input.map(analyze_input))
    }

    pub(crate) fn get_input(&self, id: &str) -> Result<Option<RunInput>, rusqlite::Error> {
        let connection = self.connection.lock().expect("database lock");
        let mut statement = connection.prepare("SELECT input_json FROM runs WHERE id = ?1")?;
        let mut rows = statement.query(params![id])?;
        if let Some(row) = rows.next()? {
            let payload: String = row.get(0)?;
            let input: RunInput = serde_json::from_str(&payload).expect("stored JSON is valid");
            Ok(Some(input))
        } else {
            Ok(None)
        }
    }

    pub fn delete_run(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let connection = self.connection.lock().expect("database lock");
        let count = connection.execute("DELETE FROM runs WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    pub fn clear(&self) -> Result<usize, rusqlite::Error> {
        let connection = self.connection.lock().expect("database lock");
        connection.execute("DELETE FROM runs", [])
    }

    pub fn export(&self) -> Result<ExportFile, rusqlite::Error> {
        Ok(ExportFile {
            schema_version: "concordia-crossing-export-v1".to_string(),
            equation_version: crate::constants::EQUATION_VERSION.to_string(),
            runs: self.list_inputs()?,
        })
    }

    pub fn import(&self, export: ExportFile) -> Result<usize, rusqlite::Error> {
        let mut inserted = 0;
        for run in export.runs {
            if self.upsert_run(run)?.is_some() {
                inserted += 1;
            }
        }
        Ok(inserted)
    }
}
