use anyhow::Result;
use duckdb::{params, Connection, Row};
use thiserror::Error;

#[cfg(feature = "gui")]
use dioxus::prelude::*;

/// Custom error types for the library
#[derive(Error, Debug)]
pub enum DuckTableError {
    #[error("DuckDB error: {0}")]
    DatabaseError(#[from] duckdb::Error),

    #[error("Table formatting error: {0}")]
    FormattingError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Main struct for executing queries and displaying results
pub struct DuckTable {
    connection: Connection,
}

/// Result of a query execution (used for GUI mode)
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    pub column_names: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl DuckTable {
    /// Create a new DuckTable with an in-memory database
    pub fn new() -> Result<Self> {
        Ok(Self {
            connection: Connection::open_in_memory()?,
        })
    }

    /// Create a new DuckTable with a file-based database
    pub fn with_file(path: &str) -> Result<Self> {
        Ok(Self {
            connection: Connection::open(path)?,
        })
    }

    /// Create with existing connection
    pub fn with_connection(connection: Connection) -> Self {
        Self { connection }
    }

    /// Execute a SQL query and return formatted table as string
    pub fn query(&self, sql: &str) -> Result<String> {
        let result = self.query_raw(sql)?;
        Ok(format!("Query returned {} rows", result.rows.len()))
    }

    /// Execute a SQL query and return raw QueryResult (for GUI or custom processing)
    pub fn query_raw(&self, sql: &str) -> Result<QueryResult> {
        let mut stmt = self.connection.prepare(sql)?;
        let mut rows = stmt.query(params![])?;

        let stmt_ref = rows
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Failed to get statement reference"))?;

        let column_count = stmt_ref.column_count();
        let mut column_names = Vec::new();

        for i in 0..column_count {
            let name = stmt_ref
                .column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col_{}", i));
            column_names.push(name);
        }

        if column_count == 0 {
            return Ok(QueryResult {
                column_names: vec![],
                rows: vec![],
            });
        }

        let mut all_rows = Vec::new();

        while let Some(row) = rows.next()? {
            let mut row_values = Vec::new();

            for i in 0..column_count {
                let value = self.extract_value_from_row(row, i);
                row_values.push(value);
            }

            all_rows.push(row_values);
        }

        Ok(QueryResult {
            column_names,
            rows: all_rows,
        })
    }

    /// Extract value from a row safely
    fn extract_value_from_row(&self, row: &Row, index: usize) -> String {
        use duckdb::types::ValueRef;

        match row.get_ref(index) {
            Ok(value_ref) => match value_ref {
                ValueRef::Null => "NULL".to_string(),
                ValueRef::Boolean(b) => b.to_string(),
                ValueRef::TinyInt(i) => i.to_string(),
                ValueRef::SmallInt(i) => i.to_string(),
                ValueRef::Int(i) => i.to_string(),
                ValueRef::BigInt(i) => i.to_string(),
                ValueRef::HugeInt(i) => i.to_string(),
                ValueRef::UTinyInt(i) => i.to_string(),
                ValueRef::USmallInt(i) => i.to_string(),
                ValueRef::UInt(i) => i.to_string(),
                ValueRef::UBigInt(i) => i.to_string(),
                ValueRef::Float(f) => {
                    if f.fract() == 0.0 && f.abs() < 1e10 {
                        format!("{:.0}", f)
                    } else {
                        format!("{:.2}", f)
                    }
                }
                ValueRef::Double(d) => {
                    if d.fract() == 0.0 && d.abs() < 1e10 {
                        format!("{:.0}", d)
                    } else {
                        format!("{:.2}", d)
                    }
                }
                ValueRef::Decimal(decimal) => {
                    format!("{:.2}", decimal.to_string().parse::<f64>().unwrap_or(0.0))
                }
                ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).to_string(),
                ValueRef::Blob(bytes) => {
                    if let Ok(s) = std::str::from_utf8(bytes) {
                        s.to_string()
                    } else {
                        format!("<blob {} bytes>", bytes.len())
                    }
                }
                ValueRef::Date32(days) => {
                    format!("Date({})", days)
                }
                ValueRef::Timestamp(_, micros) => {
                    format!("Timestamp({})", micros)
                }
                ValueRef::Time64(_, nanos) => {
                    format!("Time({})", nanos)
                }
                _ => format!("{:?}", value_ref),
            },
            Err(_) => {
                if let Ok(s) = row.get::<_, String>(index) {
                    s
                } else if let Ok(Some(s)) = row.get::<_, Option<String>>(index) {
                    s
                } else if let Ok(None::<String>) = row.get::<_, Option<String>>(index) {
                    "NULL".to_string()
                } else {
                    "?".to_string()
                }
            }
        }
    }

    /// Execute multiple queries and display all results
    pub fn query_multiple(&self, queries: &[&str]) -> Result<Vec<String>> {
        let mut results = Vec::new();
        for query in queries {
            results.push(self.query(query)?);
        }
        Ok(results)
    }

    /// Get query execution plan
    pub fn explain(&self, sql: &str) -> Result<String> {
        let explain_sql = format!("EXPLAIN {}", sql);
        self.query(&explain_sql)
    }

    /// Get query execution plan with analyze
    pub fn explain_analyze(&self, sql: &str) -> Result<String> {
        let explain_sql = format!("EXPLAIN ANALYZE {}", sql);
        self.query(&explain_sql)
    }
}

// ============================================================================
// GUI Components (enabled with "gui" feature) - Dioxus Implementation
// ============================================================================

#[cfg(feature = "gui")]
#[component]
fn DuckDbViewer() -> Element {
    let mut query_text = use_signal(|| String::new());
    let mut query_result = use_signal(|| None::<QueryResult>);
    let mut error_message = use_signal(|| None::<String>);

    let execute_query = move |_| {
        let query = query_text.read().clone();
        error_message.set(None);

        match DuckTable::new() {
            Ok(duck_table) => match duck_table.query_raw(&query) {
                Ok(result) => {
                    query_result.set(Some(result));
                }
                Err(e) => {
                    error_message.set(Some(e.to_string()));
                    query_result.set(None);
                }
            },
            Err(e) => {
                error_message.set(Some(format!("Failed to create DuckTable: {}", e)));
                query_result.set(None);
            }
        }
    };

    let clear_all = move |_| {
        query_text.set(String::new());
        query_result.set(None);
        error_message.set(None);
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; font-family: sans-serif;",
            
            // Top Panel
            div {
                style: "display: flex; justify-content: space-between; align-items: center; padding: 16px; background-color: #f5f5f5; border-bottom: 1px solid #ddd;",
                h1 { style: "margin: 0;", "🦆 DuckDB Query Viewer" }
                button {
                    style: "padding: 8px 16px; cursor: pointer;",
                    onclick: clear_all,
                    "Clear"
                }
            }

            // Main Content
            div {
                style: "flex: 1; padding: 16px; overflow: auto;",
                
                // Query Input Section
                div {
                    style: "margin-bottom: 16px;",
                    div {
                        style: "display: flex; align-items: center; gap: 8px; margin-bottom: 8px;",
                        label { "SQL Query:" }
                        button {
                            style: "padding: 6px 12px; cursor: pointer; background-color: #4CAF50; color: white; border: none; border-radius: 4px;",
                            onclick: execute_query,
                            "▶ Execute"
                        }
                    }
                    textarea {
                        style: "width: 100%; min-height: 150px; font-family: monospace; padding: 8px; border: 1px solid #ccc; border-radius: 4px;",
                        value: "{query_text}",
                        oninput: move |evt| query_text.set(evt.value().clone()),
                        placeholder: "Enter SQL query here..."
                    }
                }

                hr { style: "margin: 20px 0;" }

                // Error Display
                if let Some(error) = error_message.read().as_ref() {
                    div {
                        style: "color: red; margin-bottom: 16px; padding: 8px; background-color: #ffebee; border-radius: 4px;",
                        "❌ Error: {error}"
                    }
                }

                // Results Display
                if let Some(result) = query_result.read().as_ref() {
                    div {
                        div {
                            style: "margin-bottom: 8px; font-weight: bold;",
                            "📊 Results: {result.rows.len()} rows × {result.column_names.len()} columns"
                        }
                        hr { style: "margin: 12px 0;" }
                        div {
                            style: "overflow: auto;",
                            ResultsTable { result: result.clone() }
                        }
                    }
                } else {
                    div {
                        style: "display: flex; justify-content: center; align-items: center; height: 200px; color: #999;",
                        "Execute a query to see results"
                    }
                }
            }
        }
    }
}

#[cfg(feature = "gui")]
#[component]
fn ResultsTable(result: QueryResult) -> Element {
    rsx! {
        table {
            style: "border-collapse: collapse; width: 100%; background-color: white;",
            thead {
                tr {
                    style: "background-color: #f0f0f0;",
                    for col_name in &result.column_names {
                        th {
                            style: "border: 1px solid #ddd; padding: 12px; text-align: left; font-weight: bold;",
                            "{col_name}"
                        }
                    }
                }
            }
            tbody {
                for (idx, row) in result.rows.iter().enumerate() {
                    tr {
                        style: if idx % 2 == 0 { "background-color: #fafafa;" } else { "" },
                        for cell_value in row {
                            td {
                                style: "border: 1px solid #ddd; padding: 8px;",
                                "{cell_value}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Launch the DuckDB Query Viewer GUI (requires "gui" feature)
#[cfg(feature = "gui")]
pub fn launch_gui() -> Result<()> {
    dioxus::launch(DuckDbViewer);
    Ok(())
}

// ============================================================================
// Simple Table Viewer with Tabs
// ============================================================================

#[cfg(feature = "gui")]
#[derive(Clone, PartialEq)]
pub struct QueryTab {
    pub name: String,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
}

#[cfg(feature = "gui")]
#[component]
fn SimpleTableViewer(sql: String) -> Element {
    let tabs = use_signal(|| {
        println!("SimpleTableViewer initializing with SQL ({} bytes)", sql.len());
        execute_queries(&sql)
    });
    let mut selected_tab = use_signal(|| 0usize);

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; font-family: sans-serif;",
            
            if tabs.read().is_empty() {
                div {
                    style: "display: flex; justify-content: center; align-items: center; height: 100vh; color: #999;",
                    "⚠️ No queries to display. Please provide a SQL query."
                }
            } else {
                // Tab Bar
                if tabs.read().len() > 1 {
                    div {
                        style: "display: flex; gap: 4px; padding: 8px; background-color: #f5f5f5; border-bottom: 1px solid #ddd;",
                        for (idx, tab) in tabs.read().iter().enumerate() {
                            button {
                                style: if *selected_tab.read() == idx {
                                    "padding: 8px 16px; cursor: pointer; background-color: white; border: 1px solid #ddd; border-bottom: none; border-radius: 4px 4px 0 0;"
                                } else {
                                    "padding: 8px 16px; cursor: pointer; background-color: #e0e0e0; border: 1px solid #ddd; border-radius: 4px 4px 0 0;"
                                },
                                onclick: move |_| selected_tab.set(idx),
                                "{tab.name}"
                            }
                        }
                    }
                }

                // Tab Content
                div {
                    style: "flex: 1; padding: 16px; overflow: auto;",
                    if let Some(current_tab) = tabs.read().get(*selected_tab.read()) {
                        if let Some(error) = &current_tab.error {
                            div {
                                style: "color: #d32f2f; padding: 16px; background-color: #ffebee; border-left: 4px solid #d32f2f; border-radius: 4px;",
                                h3 { style: "margin-top: 0;", "❌ Error in {current_tab.name}" }
                                pre {
                                    style: "white-space: pre-wrap; word-wrap: break-word; font-family: monospace; margin-top: 12px;",
                                    "{error}"
                                }
                            }
                        } else if let Some(result) = &current_tab.result {
                            div {
                                h2 {
                                    style: "margin-top: 0; color: #1976d2;",
                                    "📊 {current_tab.name} Results"
                                }
                                div {
                                    style: "margin: 12px 0; color: #666;",
                                    "{result.rows.len()} rows × {result.column_names.len()} columns"
                                }
                                hr { style: "margin: 16px 0; border: none; border-top: 1px solid #e0e0e0;" }
                                div {
                                    style: "overflow: auto;",
                                    ResultsTable { result: result.clone() }
                                }
                            }
                        } else {
                            div {
                                style: "display: flex; justify-content: center; align-items: center; height: 200px; color: #999;",
                                "⚠️ No results available"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "gui")]
fn execute_queries(sql: &str) -> Vec<QueryTab> {
    let mut tabs = Vec::new();

    // 空のクエリチェック
    if sql.trim().is_empty() {
        tabs.push(QueryTab {
            name: "Error".to_string(),
            result: None,
            error: Some("No query provided. Please provide a SQL query.".to_string()),
        });
        return tabs;
    }

    let duck_table = match DuckTable::new() {
        Ok(dt) => dt,
        Err(e) => {
            tabs.push(QueryTab {
                name: "Error".to_string(),
                result: None,
                error: Some(format!("Failed to create DuckTable: {}", e)),
            });
            return tabs;
        }
    };

    // Split by semicolon and execute multiple queries
    let queries: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    println!("Found {} queries to execute", queries.len());

    if queries.is_empty() {
        tabs.push(QueryTab {
            name: "Error".to_string(),
            result: None,
            error: Some("No valid queries found. Please check your SQL syntax.".to_string()),
        });
        return tabs;
    }

    for (idx, query) in queries.iter().enumerate() {
        let tab_name = format!("Query {}", idx + 1);
        println!("Executing query {}: {} bytes", idx + 1, query.len());

        match duck_table.query_raw(query) {
            Ok(result) => {
                println!("Query {} succeeded: {} rows", idx + 1, result.rows.len());
                tabs.push(QueryTab {
                    name: tab_name,
                    result: Some(result),
                    error: None,
                });
            }
            Err(e) => {
                println!("Query {} failed: {}", idx + 1, e);
                tabs.push(QueryTab {
                    name: tab_name,
                    result: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    tabs
}

// Global SQL storage for simple GUI mode
#[cfg(feature = "gui")]
static SQL_STORAGE: std::sync::OnceLock<std::sync::Mutex<String>> = std::sync::OnceLock::new();

/// Launch a simple GUI that only shows query results (requires "gui" feature)
#[cfg(feature = "gui")]
pub fn launch_simple_gui(sql: String) -> Result<()> {
    // 空のクエリチェック
    if sql.trim().is_empty() {
        return Err(anyhow::anyhow!("No SQL query provided"));
    }
    
    // デバッグ出力
    println!("Setting SQL query ({} bytes)", sql.len());
    println!("First 100 chars: {}", &sql[..sql.len().min(100)]);
    
    // 初期化して値を設定
    if let Err(existing_value) = SQL_STORAGE.set(std::sync::Mutex::new(sql.clone())) {
        // Already initialized, this shouldn't happen in normal usage
        eprintln!("Warning: SQL_STORAGE already initialized");
        // Try to update the existing value
        if let Some(storage) = SQL_STORAGE.get() {
            if let Ok(mut guard) = storage.lock() {
                *guard = sql;
                println!("Updated existing SQL storage with new query");
            }
        }
    }
    
    dioxus::launch(SimpleTableViewerApp);
    Ok(())
}

#[cfg(feature = "gui")]
#[component]
fn SimpleTableViewerApp() -> Element {
    let sql = SQL_STORAGE
        .get()
        .map(|storage| {
            match storage.lock() {
                Ok(guard) => {
                    let sql_content = guard.clone();
                    println!("Retrieved SQL query ({} bytes)", sql_content.len());
                    sql_content
                }
                Err(e) => {
                    eprintln!("Error: Failed to lock SQL_STORAGE: {}", e);
                    String::new()
                }
            }
        })
        .unwrap_or_else(|| {
            eprintln!("Error: SQL_STORAGE not initialized - this should not happen");
            String::new()
        });
    
    if sql.is_empty() {
        return rsx! {
            div {
                style: "display: flex; justify-content: center; align-items: center; height: 100vh; color: #d32f2f; font-family: sans-serif;",
                div {
                    style: "text-align: center; padding: 32px; background-color: #ffebee; border-radius: 8px; border: 2px solid #d32f2f;",
                    h1 { style: "margin-top: 0;", "⚠️ Initialization Error" }
                    p { "SQL query was not properly initialized." }
                    p { style: "font-size: 0.9em; color: #666;", "Please check the application logs for more details." }
                }
            }
        };
    }
    
    rsx! {
        SimpleTableViewer { sql: sql }
    }
}