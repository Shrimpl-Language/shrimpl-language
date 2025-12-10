// src/orm.rs
//
// Very small ORM layer for Shrimpl models.
//
// - Uses rusqlite with a single file database `shrimpl.db` in the CWD.
// - At startup, `init_global_orm` walks all Program.models and issues
//   CREATE TABLE IF NOT EXISTS statements.
// - Exposes helpers that operate on JSON strings so the interpreter can
//   wire them into builtins without depending on internal Value types.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use once_cell::sync::Lazy;
use rusqlite::{params, Connection};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::parser::ast::{ModelDef, ModelField, Program};

/// Global ORM handle. Initialized once at startup via `init_global_orm`.
static GLOBAL_ORM: Lazy<Mutex<Option<Orm>>> = Lazy::new(|| Mutex::new(None));

/// SQLite-backed ORM for Shrimpl `model` declarations.
pub struct Orm {
    /// SQLite connection (currently a single file `shrimpl.db` in CWD).
    conn: Connection,
    /// All models keyed by model name (e.g. "User").
    models: HashMap<String, ModelDef>,
}

impl Orm {
    /// Construct a new ORM instance and run migrations for all models.
    pub fn new(conn: Connection, models: HashMap<String, ModelDef>) -> rusqlite::Result<Self> {
        let mut orm = Orm { conn, models };
        orm.migrate_all()?;
        Ok(orm)
    }

    /// Run `CREATE TABLE IF NOT EXISTS` for every model in `self.models`.
    fn migrate_all(&mut self) -> rusqlite::Result<()> {
        // Avoid borrowing `self.models` for the whole loop while also
        // mutably borrowing `self` in `migrate_model` by cloning the
        // model definitions into a local Vec first.
        let models: Vec<ModelDef> = self.models.values().cloned().collect();
        for model in &models {
            self.migrate_model(model)?;
        }
        Ok(())
    }

    /// Build and execute a `CREATE TABLE IF NOT EXISTS` statement for a
    /// single model.
    fn migrate_model(&mut self, model: &ModelDef) -> rusqlite::Result<()> {
        let mut cols = Vec::new();
        for field in &model.fields {
            cols.push(self.column_sql(field));
        }

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            model.table_name,
            cols.join(", ")
        );
        self.conn.execute(&sql, [])?;
        Ok(())
    }

    /// Convert a `ModelField` into a column definition snippet for SQLite.
    ///
    /// Handles:
    /// - type mapping (int/string/bool/etc.)
    /// - primary key
    /// - NOT NULL if the field is not optional
    fn column_sql(&self, field: &ModelField) -> String {
        let sql_ty = match field.ty.to_ascii_lowercase().as_str() {
            "int" | "integer" => "INTEGER",
            "number" | "float" | "double" | "real" => "REAL",
            "bool" | "boolean" => "INTEGER",
            "string" | "text" => "TEXT",
            other => {
                eprintln!("[shrimpl-orm] unknown field type '{}', using TEXT", other);
                "TEXT"
            }
        };

        let mut parts = vec![field.name.clone(), sql_ty.to_string()];

        if field.is_primary_key {
            parts.push("PRIMARY KEY".to_string());
        }

        if !field.is_optional {
            parts.push("NOT NULL".to_string());
        }

        parts.join(" ")
    }

    /// Insert a JSON object into the table backing `model_name`.
    ///
    /// - `record` must be a JSON object.
    /// - Only fields present in the model definition are considered.
    /// - Returns the last_insert_rowid() on success.
    pub fn insert_json(&self, model_name: &str, record: &JsonValue) -> Result<i64, String> {
        let model = self
            .models
            .get(model_name)
            .ok_or_else(|| format!("unknown model '{}'", model_name))?;

        let obj = record
            .as_object()
            .ok_or_else(|| "record must be a JSON object".to_string())?;

        let mut cols = Vec::new();
        let mut placeholders = Vec::new();
        let mut values: Vec<JsonValue> = Vec::new();

        for field in &model.fields {
            if let Some(value) = obj.get(&field.name) {
                cols.push(field.name.clone());
                placeholders.push("?".to_string());
                values.push(value.clone());
            }
        }

        if cols.is_empty() {
            return Err("record has no matching fields".to_string());
        }

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            model.table_name,
            cols.join(", "),
            placeholders.join(", ")
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| format!("insert prepare failed: {e}"))?;

        let params_vec: Vec<rusqlite::types::Value> =
            values.into_iter().map(json_to_sql_value).collect();

        let rows_changed = stmt
            .execute(rusqlite::params_from_iter(params_vec.iter()))
            .map_err(|e| format!("insert execute failed: {e}"))?;

        if rows_changed == 0 {
            return Err("insert affected 0 rows".to_string());
        }

        Ok(self.conn.last_insert_rowid())
    }

    /// Look up a row by primary key in `model_name`.
    ///
    /// - `id` is a JSON scalar (number/string/bool) representing the PK.
    /// - Returns `Ok(Some(JsonValue))` if found, or `Ok(None)` otherwise.
    pub fn find_by_id(
        &self,
        model_name: &str,
        id: &JsonValue,
    ) -> Result<Option<JsonValue>, String> {
        let model = self
            .models
            .get(model_name)
            .ok_or_else(|| format!("unknown model '{}'", model_name))?;

        let pk_field = model
            .fields
            .iter()
            .find(|f| f.is_primary_key)
            .ok_or_else(|| format!("model '{}' has no primary key field", model_name))?;

        let sql = format!(
            "SELECT * FROM {} WHERE {} = ? LIMIT 1",
            model.table_name, pk_field.name
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| format!("find prepare failed: {e}"))?;

        let id_value = json_to_sql_value(id.clone());
        let mut rows = stmt
            .query(params![id_value])
            .map_err(|e| format!("find query failed: {e}"))?;

        if let Some(row) = rows.next().map_err(|e| format!("find next failed: {e}"))? {
            let mut obj = JsonMap::new();
            for field in &model.fields {
                let val: rusqlite::types::Value = row
                    .get(field.name.as_str())
                    .map_err(|e| format!("column get failed: {e}"))?;
                obj.insert(field.name.clone(), sql_value_to_json(val));
            }
            Ok(Some(JsonValue::Object(obj)))
        } else {
            Ok(None)
        }
    }
}

/// Convert a JSON value into a rusqlite `Value`.
fn json_to_sql_value(v: JsonValue) -> rusqlite::types::Value {
    use rusqlite::types::Value as SqlValue;
    match v {
        JsonValue::Null => SqlValue::Null,
        JsonValue::Bool(b) => SqlValue::Integer(if b { 1 } else { 0 }),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                SqlValue::Real(f)
            } else {
                SqlValue::Text(n.to_string())
            }
        }
        JsonValue::String(s) => SqlValue::Text(s),
        other => SqlValue::Text(other.to_string()),
    }
}

/// Convert a rusqlite `Value` into JSON.
fn sql_value_to_json(v: rusqlite::types::Value) -> JsonValue {
    use rusqlite::types::Value as SqlValue;
    match v {
        SqlValue::Null => JsonValue::Null,
        SqlValue::Integer(i) => JsonValue::from(i),
        SqlValue::Real(f) => JsonValue::from(f),
        SqlValue::Text(s) => JsonValue::from(s),
        SqlValue::Blob(b) => JsonValue::from(STANDARD.encode(b)),
    }
}

/// Initialize the global ORM from the Program's models.
/// Called from main.rs when starting the server.
pub fn init_global_orm(program: &Program) -> rusqlite::Result<()> {
    // Open (or create) the SQLite database in the current working directory.
    let conn = Connection::open("shrimpl.db")?;
    let models = program.models.clone();
    let orm = Orm::new(conn, models)?;

    let mut guard = GLOBAL_ORM.lock().expect("GLOBAL_ORM poisoned");
    *guard = Some(orm);

    Ok(())
}

/// Insert a record into `model_name`.
///
/// - `record_json` must be a JSON object string.
/// - Returns the rowid (or PK) as a string on success.
pub fn orm_insert(model_name: &str, record_json: &str) -> Result<String, String> {
    let guard = GLOBAL_ORM
        .lock()
        .map_err(|_| "GLOBAL_ORM poisoned".to_string())?;
    let orm = guard
        .as_ref()
        .ok_or_else(|| "ORM not initialized".to_string())?;

    let value: JsonValue =
        serde_json::from_str(record_json).map_err(|e| format!("invalid JSON: {e}"))?;

    let rowid = orm.insert_json(model_name, &value)?;
    Ok(rowid.to_string())
}

/// Find a record by primary key.
///
/// - `id_json` can be a JSON scalar (number/string/bool).
/// - Returns Some(JSON string) or None.
pub fn orm_find_by_id(model_name: &str, id_json: &str) -> Result<Option<String>, String> {
    let guard = GLOBAL_ORM
        .lock()
        .map_err(|_| "GLOBAL_ORM poisoned".to_string())?;
    let orm = guard
        .as_ref()
        .ok_or_else(|| "ORM not initialized".to_string())?;

    let id_val: JsonValue =
        serde_json::from_str(id_json).map_err(|e| format!("invalid id JSON: {e}"))?;

    let result = orm.find_by_id(model_name, &id_val)?;
    Ok(result.map(|v| v.to_string()))
}
