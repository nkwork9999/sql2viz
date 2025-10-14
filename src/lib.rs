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
const PLOTLY_JS: &str = include_str!("../assets/plotly-2.27.0.min.js");

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
            
            div {
                style: "display: flex; justify-content: space-between; align-items: center; padding: 16px; background-color: #f5f5f5; border-bottom: 1px solid #ddd;",
                h1 { style: "margin: 0;", "🦆 DuckDB Query Viewer" }
                button {
                    style: "padding: 8px 16px; cursor: pointer;",
                    onclick: clear_all,
                    "Clear"
                }
            }

            div {
                style: "flex: 1; padding: 16px; overflow: auto;",
                
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

                if let Some(error) = error_message.read().as_ref() {
                    div {
                        style: "color: red; margin-bottom: 16px; padding: 8px; background-color: #ffebee; border-radius: 4px;",
                        "❌ Error: {error}"
                    }
                }

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
                    {
                        let row_style = if idx % 2 == 0 { "background-color: #fafafa;" } else { "" };
                        rsx! {
                            tr {
                                style: "{row_style}",
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
#[derive(Clone, Copy, PartialEq)]
pub enum ChartType {
    Bar,
    Line,
    Area,
    Scatter,
}

#[cfg(feature = "gui")]
impl ChartType {
    fn as_str(&self) -> &str {
        match self {
            ChartType::Bar => "bar",
            ChartType::Line => "line",
            ChartType::Area => "area",
            ChartType::Scatter => "scatter",
        }
    }
    
    fn icon(&self) -> &str {
        match self {
            ChartType::Bar => "📊",
            ChartType::Line => "📈",
            ChartType::Area => "📉",
            ChartType::Scatter => "🔵",
        }
    }
    
    fn label(&self) -> &str {
        match self {
            ChartType::Bar => "Bar",
            ChartType::Line => "Line",
            ChartType::Area => "Area",
            ChartType::Scatter => "Scatter",
        }
    }
}

#[cfg(feature = "gui")]
#[component]
fn SimpleTableViewer(sql: String) -> Element {
    let tabs = use_signal(|| execute_queries(&sql));
    let mut selected_tab = use_signal(|| 0usize);
    let mut view_mode = use_signal(|| "table");
    let mut plotly_loaded = use_signal(|| false);
    let mut chart_type = use_signal(|| ChartType::Bar);

    use_effect(move || {
        spawn(async move {
            let plotly_escaped = PLOTLY_JS
                .replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace("</script>", "<\\/script>");
            
            let inject_js = format!(
                r#"
                (function() {{
                    if (typeof Plotly !== 'undefined') {{
                        dioxus.send('READY');
                        return;
                    }}
                    
                    try {{
                        var script = document.createElement('script');
                        script.type = 'text/javascript';
                        script.textContent = `{}`;
                        document.head.appendChild(script);
                        
                        if (typeof Plotly !== 'undefined') {{
                            dioxus.send('READY');
                        }} else {{
                            dioxus.send('ERROR');
                        }}
                    }} catch (e) {{
                        dioxus.send('ERROR');
                    }}
                }})()
                "#,
                plotly_escaped
            );
            
            let mut eval = document::eval(&inject_js);
            if let Ok(status) = eval.recv::<String>().await {
                if status == "READY" {
                    plotly_loaded.set(true);
                }
            }
        });
    });

    let is_empty = tabs.read().is_empty();
    let has_multiple_tabs = tabs.read().len() > 1;
    let current_tab_data = tabs.read().get(*selected_tab.read()).cloned();
    let current_view_mode = *view_mode.read();
    let is_plotly_loaded = *plotly_loaded.read();
    let current_chart_type = *chart_type.read();
    
    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; font-family: sans-serif;",
            
            if is_empty {
                div {
                    style: "display: flex; justify-content: center; align-items: center; height: 100vh; color: #999;",
                    "⚠️ No queries to display"
                }
            } else {
                if has_multiple_tabs {
                    div {
                        style: "display: flex; gap: 4px; padding: 8px; background-color: #f5f5f5; border-bottom: 1px solid #ddd;",
                        for (idx, tab) in tabs.read().iter().enumerate() {
                            {
                                let is_selected = *selected_tab.read() == idx;
                                let btn_style = if is_selected {
                                    "padding: 8px 16px; cursor: pointer; background-color: white; border: 1px solid #ddd; border-bottom: none; border-radius: 4px 4px 0 0;"
                                } else {
                                    "padding: 8px 16px; cursor: pointer; background-color: #e0e0e0; border: 1px solid #ddd; border-radius: 4px 4px 0 0;"
                                };
                                rsx! {
                                    button {
                                        key: "{idx}",
                                        style: "{btn_style}",
                                        onclick: move |_| selected_tab.set(idx),
                                        "{tab.name}"
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    style: "flex: 1; padding: 16px; overflow: auto;",
                    if let Some(current_tab) = current_tab_data {
                        if let Some(error) = current_tab.error {
                            div {
                                style: "color: #d32f2f; padding: 16px; background-color: #ffebee; border-left: 4px solid #d32f2f; border-radius: 4px;",
                                h3 { style: "margin-top: 0;", "❌ Error in {current_tab.name}" }
                                pre {
                                    style: "white-space: pre-wrap; word-wrap: break-word; font-family: monospace; margin-top: 12px;",
                                    "{error}"
                                }
                            }
                        } else if let Some(result) = current_tab.result {
                            div {
                                div {
                                    style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                                    div {
                                        h2 {
                                            style: "margin: 0; color: #1976d2;",
                                            "📊 {current_tab.name} Results"
                                        }
                                        div {
                                            style: "margin-top: 8px; color: #666; font-size: 0.9em;",
                                            "{result.rows.len()} rows × {result.column_names.len()} columns"
                                        }
                                    }
                                    div {
                                        style: "display: flex; gap: 8px;",
                                        {
                                            let table_style = if current_view_mode == "table" {
                                                "padding: 8px 16px; cursor: pointer; background-color: #1976d2; color: white; border: none; border-radius: 4px;"
                                            } else {
                                                "padding: 8px 16px; cursor: pointer; background-color: #e0e0e0; color: #333; border: none; border-radius: 4px;"
                                            };
                                            rsx! {
                                                button {
                                                    style: "{table_style}",
                                                    onclick: move |_| view_mode.set("table"),
                                                    "📋 Table"
                                                }
                                            }
                                        }
                                        {
                                            let chart_style = if current_view_mode == "chart" {
                                                "padding: 8px 16px; cursor: pointer; background-color: #1976d2; color: white; border: none; border-radius: 4px;"
                                            } else {
                                                "padding: 8px 16px; cursor: pointer; background-color: #e0e0e0; color: #333; border: none; border-radius: 4px;"
                                            };
                                            rsx! {
                                                button {
                                                    style: "{chart_style}",
                                                    onclick: move |_| view_mode.set("chart"),
                                                    disabled: !is_plotly_loaded,
                                                    "📊 Chart"
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // Chart type selector (only visible in chart mode)
                                if current_view_mode == "chart" && is_plotly_loaded {
                                    div {
                                        style: "margin-bottom: 16px; padding: 12px; background-color: #f5f5f5; border-radius: 4px;",
                                        div {
                                            style: "margin-bottom: 8px; font-weight: bold; color: #555;",
                                            "Chart Type:"
                                        }
                                        div {
                                            style: "display: flex; gap: 12px; flex-wrap: wrap;",
                                            
                                            // Bar Chart
                                            {
                                                let bar_border = if current_chart_type == ChartType::Bar { "#1976d2" } else { "#ddd" };
                                                let bar_style = format!("display: flex; align-items: center; gap: 6px; cursor: pointer; padding: 6px 12px; background-color: white; border-radius: 4px; border: 2px solid {}; transition: all 0.2s;", bar_border);
                                                rsx! {
                                                    label {
                                                        style: "{bar_style}",
                                                        input {
                                                            r#type: "radio",
                                                            name: "chart-type",
                                                            checked: current_chart_type == ChartType::Bar,
                                                            onchange: move |_| chart_type.set(ChartType::Bar),
                                                        }
                                                        span { "{ChartType::Bar.icon()} {ChartType::Bar.label()}" }
                                                    }
                                                }
                                            }
                                            
                                            // Line Chart
                                            {
                                                let line_border = if current_chart_type == ChartType::Line { "#1976d2" } else { "#ddd" };
                                                let line_style = format!("display: flex; align-items: center; gap: 6px; cursor: pointer; padding: 6px 12px; background-color: white; border-radius: 4px; border: 2px solid {}; transition: all 0.2s;", line_border);
                                                rsx! {
                                                    label {
                                                        style: "{line_style}",
                                                        input {
                                                            r#type: "radio",
                                                            name: "chart-type",
                                                            checked: current_chart_type == ChartType::Line,
                                                            onchange: move |_| chart_type.set(ChartType::Line),
                                                        }
                                                        span { "{ChartType::Line.icon()} {ChartType::Line.label()}" }
                                                    }
                                                }
                                            }
                                            
                                            // Area Chart
                                            {
                                                let area_border = if current_chart_type == ChartType::Area { "#1976d2" } else { "#ddd" };
                                                let area_style = format!("display: flex; align-items: center; gap: 6px; cursor: pointer; padding: 6px 12px; background-color: white; border-radius: 4px; border: 2px solid {}; transition: all 0.2s;", area_border);
                                                rsx! {
                                                    label {
                                                        style: "{area_style}",
                                                        input {
                                                            r#type: "radio",
                                                            name: "chart-type",
                                                            checked: current_chart_type == ChartType::Area,
                                                            onchange: move |_| chart_type.set(ChartType::Area),
                                                        }
                                                        span { "{ChartType::Area.icon()} {ChartType::Area.label()}" }
                                                    }
                                                }
                                            }
                                            
                                            // Scatter Plot
                                            {
                                                let scatter_border = if current_chart_type == ChartType::Scatter { "#1976d2" } else { "#ddd" };
                                                let scatter_style = format!("display: flex; align-items: center; gap: 6px; cursor: pointer; padding: 6px 12px; background-color: white; border-radius: 4px; border: 2px solid {}; transition: all 0.2s;", scatter_border);
                                                rsx! {
                                                    label {
                                                        style: "{scatter_style}",
                                                        input {
                                                            r#type: "radio",
                                                            name: "chart-type",
                                                            checked: current_chart_type == ChartType::Scatter,
                                                            onchange: move |_| chart_type.set(ChartType::Scatter),
                                                        }
                                                        span { "{ChartType::Scatter.icon()} {ChartType::Scatter.label()}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                hr { style: "margin: 16px 0; border: none; border-top: 1px solid #e0e0e0;" }
                                
                                if current_view_mode == "table" {
                                    div {
                                        style: "overflow: auto;",
                                        ResultsTable { result: result.clone() }
                                    }
                                } else if is_plotly_loaded {
                                    ChartView { 
                                        result: result.clone(),
                                        tab_index: *selected_tab.read(),
                                        chart_type: current_chart_type
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "gui")]
#[component]
fn ChartView(result: QueryResult, tab_index: usize, chart_type: ChartType) -> Element {
    let chart_id = use_signal(move || format!("chart-{}", tab_index));
    let mut error_message = use_signal(|| None::<String>);
    
    use_effect(move || {
        let result_clone = result.clone();
        let chart_id_value = chart_id.read().clone();
        
        error_message.set(None);
        
        spawn(async move {
            let wait_for_dom_js = format!(
                r#"
                (async function() {{
                    for (let i = 0; i < 50; i++) {{
                        const element = document.getElementById('{}');
                        if (element) {{
                            dioxus.send('READY');
                            return;
                        }}
                        await new Promise(resolve => setTimeout(resolve, 50));
                    }}
                    dioxus.send('ERROR');
                }})()
                "#,
                chart_id_value
            );
            
            let mut dom_eval = document::eval(&wait_for_dom_js);
            match dom_eval.recv::<String>().await {
                Ok(status) if status == "READY" => {
                    render_chart(result_clone, chart_id_value, chart_type, error_message);
                },
                _ => {
                    error_message.set(Some("Failed to create chart container".to_string()));
                }
            }
        });
    });
    
    rsx! {
        div {
            style: "position: relative;",
            
            if let Some(error) = error_message.read().as_ref() {
                div {
                    style: "padding: 20px; background-color: #ffebee; border-left: 4px solid #d32f2f; border-radius: 4px; margin-bottom: 16px;",
                    div {
                        style: "font-weight: bold; color: #d32f2f; margin-bottom: 8px;",
                        "❌ Chart Error"
                    }
                    pre {
                        style: "white-space: pre-wrap; word-wrap: break-word; font-family: monospace; font-size: 0.9em; color: #666;",
                        "{error}"
                    }
                }
            }
            
            div {
                id: "{chart_id}",
                style: "width: 100%; height: 70vh; min-height: 500px; max-height: 900px; background-color: white; border-radius: 4px; box-shadow: 0 2px 4px rgba(0,0,0,0.1);"
            }
        }
    }
}

#[cfg(feature = "gui")]
fn render_chart(
    result: QueryResult,
    chart_id: String,
    chart_type: ChartType,
    mut error_message: Signal<Option<String>>,
) {
    let mut numeric_columns = Vec::new();
    if !result.rows.is_empty() {
        for (col_idx, col_name) in result.column_names.iter().enumerate() {
            if let Some(first_row) = result.rows.first() {
                if let Some(value) = first_row.get(col_idx) {
                    if value != "NULL" && value.parse::<f64>().is_ok() {
                        numeric_columns.push((col_idx, col_name.clone()));
                    }
                }
            }
        }
    }
    
    let label_col_idx = result.column_names.iter().enumerate()
        .find(|(idx, _)| !numeric_columns.iter().any(|(num_idx, _)| num_idx == idx))
        .map(|(idx, _)| idx);
    
    if numeric_columns.is_empty() {
        error_message.set(Some("No numeric columns found for charting".to_string()));
        return;
    }
    
    let mut traces = Vec::new();
    
    for (col_idx, col_name) in numeric_columns.iter() {
        let values: Vec<String> = result.rows.iter()
            .filter_map(|row| {
                row.get(*col_idx)
                    .and_then(|v| {
                        if v == "NULL" { None } else { Some(v.clone()) }
                    })
            })
            .collect();
        
        let col_name_escaped = col_name.replace('\\', "\\\\").replace('"', "\\\"");
        
        let x_data = if let Some(label_idx) = label_col_idx {
            let labels: Vec<String> = result.rows.iter()
                .filter_map(|row| row.get(label_idx).map(|s| {
                    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                }))
                .collect();
            format!("[{}]", labels.join(", "))
        } else {
            let indices: Vec<String> = (0..result.rows.len())
                .map(|i| format!("\"{}\"", i))
                .collect();
            format!("[{}]", indices.join(", "))
        };
        
        let trace = match chart_type {
            ChartType::Area => {
                format!(
                    r#"{{
                        type: 'scatter',
                        mode: 'lines',
                        name: "{}",
                        y: [{}],
                        x: {},
                        fill: 'tonexty',
                        fillcolor: 'rgba(31, 119, 180, 0.3)'
                    }}"#,
                    col_name_escaped,
                    values.join(", "),
                    x_data
                )
            },
            ChartType::Scatter => {
                format!(
                    r#"{{
                        type: 'scatter',
                        mode: 'markers',
                        name: "{}",
                        y: [{}],
                        x: {},
                        marker: {{ size: 10 }}
                    }}"#,
                    col_name_escaped,
                    values.join(", "),
                    x_data
                )
            },
            ChartType::Line => {
                format!(
                    r#"{{
                        type: 'scatter',
                        mode: 'lines+markers',
                        name: "{}",
                        y: [{}],
                        x: {}
                    }}"#,
                    col_name_escaped,
                    values.join(", "),
                    x_data
                )
            },
            ChartType::Bar => {
                format!(
                    r#"{{
                        type: 'bar',
                        name: "{}",
                        y: [{}],
                        x: {}
                    }}"#,
                    col_name_escaped,
                    values.join(", "),
                    x_data
                )
            }
        };
        
        traces.push(trace);
    }
    
    let x_axis_title = if let Some(label_idx) = label_col_idx {
        result.column_names.get(label_idx)
            .cloned()
            .unwrap_or_else(|| "Index".to_string())
            .replace('\\', "\\\\").replace('"', "\\\"")
    } else {
        "Index".to_string()
    };
    
    let barmode = if chart_type == ChartType::Bar { "barmode: 'group'," } else { "" };
    
    spawn(async move {
        let js_code = format!(
            r#"
            (async function() {{
                try {{
                    if (typeof Plotly === 'undefined') {{
                        throw new Error('Plotly not loaded');
                    }}
                    
                    const chartDiv = document.getElementById('{}');
                    if (!chartDiv) {{
                        throw new Error('Chart container not found');
                    }}
                    
                    Plotly.purge(chartDiv);
                    
                    const data = [{}];
                    const layout = {{
                        title: 'Query Results - {} Chart',
                        {}
                        xaxis: {{ title: "{}" }},
                        yaxis: {{ title: 'Value' }},
                        plot_bgcolor: '#f9f9f9',
                        paper_bgcolor: 'white',
                        margin: {{ l: 60, r: 40, t: 60, b: 80 }}
                    }};
                    const config = {{
                        responsive: true,
                        displayModeBar: true,
                        displaylogo: false
                    }};
                    
                    await Plotly.newPlot('{}', data, layout, config);
                    dioxus.send('SUCCESS');
                }} catch (error) {{
                    dioxus.send('ERROR: ' + error.message);
                }}
            }})()
            "#,
            chart_id,
            traces.join(", "),
            chart_type.label(),
            barmode,
            x_axis_title,
            chart_id
        );
        
        let mut eval = document::eval(&js_code);
        
        if let Ok(result) = eval.recv::<String>().await {
            if result.starts_with("ERROR") {
                error_message.set(Some(format!("Chart rendering failed: {}", result)));
            }
        }
    });
}

#[cfg(feature = "gui")]
fn execute_queries(sql: &str) -> Vec<QueryTab> {
    let mut tabs = Vec::new();

    if sql.trim().is_empty() {
        tabs.push(QueryTab {
            name: "Error".to_string(),
            result: None,
            error: Some("No query provided".to_string()),
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

    let queries: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if queries.is_empty() {
        tabs.push(QueryTab {
            name: "Error".to_string(),
            result: None,
            error: Some("No valid queries found".to_string()),
        });
        return tabs;
    }

    for (idx, query) in queries.iter().enumerate() {
        let tab_name = format!("Query {}", idx + 1);

        match duck_table.query_raw(query) {
            Ok(result) => {
                tabs.push(QueryTab {
                    name: tab_name,
                    result: Some(result),
                    error: None,
                });
            }
            Err(e) => {
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

#[cfg(feature = "gui")]
static SQL_STORAGE: std::sync::OnceLock<std::sync::Mutex<String>> = std::sync::OnceLock::new();

/// Launch a simple GUI that only shows query results (requires "gui" feature)
#[cfg(feature = "gui")]
pub fn launch_simple_gui(sql: String) -> Result<()> {
    if sql.trim().is_empty() {
        return Err(anyhow::anyhow!("No SQL query provided"));
    }
    
    if let Err(_) = SQL_STORAGE.set(std::sync::Mutex::new(sql.clone())) {
        if let Some(storage) = SQL_STORAGE.get() {
            if let Ok(mut guard) = storage.lock() {
                *guard = sql;
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
        .and_then(|storage| storage.lock().ok())
        .map(|guard| guard.clone())
        .unwrap_or_default();
    
    if sql.is_empty() {
        return rsx! {
            div {
                style: "display: flex; justify-content: center; align-items: center; height: 100vh; color: #d32f2f; font-family: sans-serif;",
                div {
                    style: "text-align: center; padding: 32px; background-color: #ffebee; border-radius: 8px; border: 2px solid #d32f2f;",
                    h1 { style: "margin-top: 0;", "⚠️ Initialization Error" }
                    p { "SQL query was not properly initialized" }
                }
            }
        };
    }
    
    rsx! {
        SimpleTableViewer { sql: sql }
    }
}