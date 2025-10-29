// src/lib.rs - SQL2VIZ完全版 (ネイティブ + WASM対応)
// VizBuilder, ChartConfig, Python bindingsすべて含む

use anyhow::Result;
use thiserror::Error;

// ============================================================================
// エラー型定義
// ============================================================================

#[derive(Error, Debug)]
pub enum DuckTableError {
    #[error("DuckDB error: {0}")]
    DatabaseError(String),

    #[error("Table formatting error: {0}")]
    FormattingError(String),

    #[error("IO error: {0}")]
    IoError(String),
}

// DuckDB Error からの変換（ネイティブのみ）
#[cfg(all(not(target_arch = "wasm32"), feature = "duckdb"))]
impl From<duckdb::Error> for DuckTableError {
    fn from(err: duckdb::Error) -> Self {
        DuckTableError::DatabaseError(err.to_string())
    }
}

// std::io::Error からの変換
impl From<std::io::Error> for DuckTableError {
    fn from(err: std::io::Error) -> Self {
        DuckTableError::IoError(err.to_string())
    }
}

// ============================================================================
// 共通データ型
// ============================================================================

/// クエリ結果を保持する構造体
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "wasm", derive(serde::Serialize, serde::Deserialize))]
pub struct QueryResult {
    pub column_names: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

// ============================================================================
// ネイティブ専用: DuckDB処理
// ============================================================================

#[cfg(all(not(target_arch = "wasm32"), feature = "duckdb"))]
use duckdb::{params, Connection, Row};

/// DuckDB接続を保持する構造体（ネイティブ専用）
#[cfg(all(not(target_arch = "wasm32"), feature = "duckdb"))]
pub struct DuckTable {
    connection: Connection,
}

#[cfg(all(not(target_arch = "wasm32"), feature = "duckdb"))]
impl DuckTable {
    /// インメモリデータベースを作成
    pub fn new() -> Result<Self> {
        Ok(Self {
            connection: Connection::open_in_memory()?,
        })
    }

    /// ファイルベースのデータベースを開く
    pub fn with_file(path: &str) -> Result<Self> {
        Ok(Self {
            connection: Connection::open(path)?,
        })
    }

    /// 既存の接続を使用
    pub fn with_connection(connection: Connection) -> Self {
        Self { connection }
    }

    /// SQLクエリを実行して文字列結果を返す
    pub fn query(&self, sql: &str) -> Result<String> {
        let result = self.query_raw(sql)?;
        Ok(format!("Query returned {} rows", result.rows.len()))
    }

    /// SQLクエリを実行してQueryResultを返す
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

    /// 行から値を安全に抽出
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

    /// 複数のクエリを実行
    pub fn query_multiple(&self, queries: &[&str]) -> Result<Vec<String>> {
        let mut results = Vec::new();
        for query in queries {
            results.push(self.query(query)?);
        }
        Ok(results)
    }

    /// クエリ実行計画を取得
    pub fn explain(&self, sql: &str) -> Result<String> {
        let explain_sql = format!("EXPLAIN {}", sql);
        self.query(&explain_sql)
    }

    /// クエリ実行計画を分析付きで取得
    pub fn explain_analyze(&self, sql: &str) -> Result<String> {
        let explain_sql = format!("EXPLAIN ANALYZE {}", sql);
        self.query(&explain_sql)
    }
}

// ============================================================================
// WASM専用: JavaScript連携
// ============================================================================

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
use wasm_bindgen::prelude::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
use serde::{Deserialize, Serialize};

/// WASM用のQueryResult（JavaScriptから呼び出し可能）
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
#[wasm_bindgen]
#[derive(Serialize, Deserialize)]
pub struct WasmQueryResult {
    column_names: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
#[wasm_bindgen]
impl WasmQueryResult {
    /// コンストラクタ
    #[wasm_bindgen(constructor)]
    pub fn new(column_names: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self { column_names, rows }
    }

    /// カラム名を取得
    #[wasm_bindgen(getter)]
    pub fn column_names(&self) -> Vec<String> {
        self.column_names.clone()
    }

    /// 行データを取得
    #[wasm_bindgen(getter)]
    pub fn rows(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.rows).unwrap_or(JsValue::NULL)
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
impl From<WasmQueryResult> for QueryResult {
    fn from(wasm_result: WasmQueryResult) -> Self {
        QueryResult {
            column_names: wasm_result.column_names,
            rows: wasm_result.rows,
        }
    }
}

/// WASM初期化
#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    // パニック時のスタックトレース表示を改善
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
    
    // ブラウザコンソールにログ出力
    web_sys::console::log_1(&"SQL2VIZ WASM initialized".into());
}

/// JavaScriptからクエリ結果を受け取ってGUIを起動（WASM専用）
#[cfg(all(target_arch = "wasm32", feature = "wasm", feature = "gui"))]
#[wasm_bindgen]
pub fn vizcreate_wasm(query_result: WasmQueryResult) -> Result<(), JsValue> {
    let result: QueryResult = query_result.into();
    
    let tabs = vec![QueryTab {
        name: "Query Result".to_string(),
        result: Some(result.clone()),
        error: None,
        view_mode: ViewMode::Table,
        chart_type: ChartType::Bar,
        x_axis_column: result.column_names.first().cloned(),
        y_axis_column: result.column_names.get(1).or_else(|| result.column_names.first()).cloned(),
    }];

    let app_state = AppState::with_tabs(tabs);

    iced::application("SQL2VIZ - WASM", AppState::update, AppState::view)
        .theme(AppState::theme)
        .run_with(move || (app_state.clone(), iced::Task::none()))
        .map_err(|e| JsValue::from_str(&format!("Failed to run app: {:?}", e)))
}

// ============================================================================
// GUI Components (Iced Implementation) - ネイティブ & WASM共通
// ============================================================================

#[cfg(feature = "gui")]
use iced::widget::{button, canvas, column, container, pick_list, row, scrollable, text};

#[cfg(feature = "gui")]
use iced::{Alignment, Color, Element, Font, Length, Point, Rectangle, Size, Task, Theme};

#[cfg(feature = "gui")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "gui")]
static SQL_STORAGE: std::sync::OnceLock<Arc<Mutex<String>>> = std::sync::OnceLock::new();

#[cfg(feature = "gui")]
#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(usize),
    ViewModeChanged(ViewMode),
    ChartTypeChanged(ChartType),
    XAxisColumnChanged(String),
    YAxisColumnChanged(String),
    None,
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    Table,
    Chart,
}

#[cfg(feature = "gui")]
impl ViewMode {
    fn all() -> Vec<ViewMode> {
        vec![ViewMode::Table, ViewMode::Chart]
    }
}

#[cfg(feature = "gui")]
impl std::fmt::Display for ViewMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewMode::Table => write!(f, "📋 Table"),
            ViewMode::Chart => write!(f, "📊 Chart"),
        }
    }
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChartType {
    Bar,
    Line,
    Area,
    Scatter,
}

#[cfg(feature = "gui")]
impl ChartType {
    pub fn all() -> Vec<ChartType> {
        vec![
            ChartType::Bar,
            ChartType::Line,
            ChartType::Area,
            ChartType::Scatter,
        ]
    }
}

#[cfg(feature = "gui")]
impl std::fmt::Display for ChartType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChartType::Bar => write!(f, "📊 Bar Chart"),
            ChartType::Line => write!(f, "📈 Line Chart"),
            ChartType::Area => write!(f, "📉 Area Chart"),
            ChartType::Scatter => write!(f, "🔵 Scatter Plot"),
        }
    }
}

#[cfg(feature = "gui")]
#[derive(Clone, PartialEq)]
pub struct QueryTab {
    pub name: String,
    pub result: Option<QueryResult>,
    pub error: Option<String>,
    pub view_mode: ViewMode,
    pub chart_type: ChartType,
    pub x_axis_column: Option<String>,
    pub y_axis_column: Option<String>,
}

#[cfg(feature = "gui")]
#[derive(Clone)]
pub struct AppState {
    pub tabs: Vec<QueryTab>,
    pub selected_tab: usize,
}

#[cfg(feature = "gui")]
impl AppState {
    pub fn new(sql: String) -> Self {
        #[cfg(all(not(target_arch = "wasm32"), feature = "duckdb"))]
        let tabs = execute_queries_with_configs(&sql);
        
        #[cfg(any(target_arch = "wasm32", not(feature = "duckdb")))]
        let tabs = vec![QueryTab {
            name: "Error".to_string(),
            result: None,
            error: Some("DuckDB not available in WASM. Use vizcreate_wasm() instead.".to_string()),
            view_mode: ViewMode::Table,
            chart_type: ChartType::Bar,
            x_axis_column: None,
            y_axis_column: None,
        }];
        
        Self::with_tabs(tabs)
    }
    
    pub fn with_tabs(tabs: Vec<QueryTab>) -> Self {
        Self {
            tabs,
            selected_tab: 0,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TabSelected(index) => {
                if index < self.tabs.len() {
                    self.selected_tab = index;
                }
            }
            Message::ViewModeChanged(mode) => {
                if let Some(tab) = self.tabs.get_mut(self.selected_tab) {
                    tab.view_mode = mode;
                }
            }
            Message::ChartTypeChanged(chart_type) => {
                if let Some(tab) = self.tabs.get_mut(self.selected_tab) {
                    tab.chart_type = chart_type;
                }
            }
            Message::XAxisColumnChanged(col) => {
                if let Some(tab) = self.tabs.get_mut(self.selected_tab) {
                    tab.x_axis_column = Some(col);
                }
            }
            Message::YAxisColumnChanged(col) => {
                if let Some(tab) = self.tabs.get_mut(self.selected_tab) {
                    tab.y_axis_column = Some(col);
                }
            }
            Message::None => {}
        }
        Task::none()
    }

    pub fn view(&self) -> Element<Message> {
        if self.tabs.is_empty() {
            return container(text("⚠️ No queries to display").color(Color::BLACK))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }

        let mut content = column![].spacing(10).padding(20);

        // Tab selector (if multiple tabs)
        if self.tabs.len() > 1 {
            let tab_buttons: Vec<Element<Message>> = self
                .tabs
                .iter()
                .enumerate()
                .map(|(idx, tab)| {
                    let btn = button(text(&tab.name))
                        .padding(10)
                        .on_press(Message::TabSelected(idx));
                    
                    Element::from(btn)
                })
                .collect();

            let tabs_row = row(tab_buttons).spacing(5);
            content = content.push(tabs_row);
        }

        // Current tab content
        if let Some(current_tab) = self.tabs.get(self.selected_tab) {
            if let Some(error) = &current_tab.error {
                let error_display = container(
                    column![
                        text(format!("❌ Error in {}", current_tab.name))
                            .size(20)
                            .color(Color::BLACK),
                        text(error).size(14).color(Color::BLACK)
                    ]
                    .spacing(10)
                )
                .padding(20)
                .style(|_theme: &Theme| {
                    container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(1.0, 0.9, 0.9))),
                        border: iced::Border {
                            color: Color::from_rgb(0.8, 0.2, 0.2),
                            width: 2.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }
                });

                content = content.push(error_display);
            } else if let Some(result) = &current_tab.result {
                // Header with view mode selector
                let header = row![
                    column![
                        text(format!("📊 {} Results", current_tab.name))
                            .size(24)
                            .color(Color::BLACK),
                        text(format!(
                            "{} rows × {} columns",
                            result.rows.len(),
                            result.column_names.len()
                        ))
                        .size(14)
                        .color(Color::BLACK)
                    ]
                    .spacing(5),
                    row![
                        pick_list(
                            ViewMode::all(),
                            Some(current_tab.view_mode.clone()),
                            Message::ViewModeChanged
                        )
                        .padding(10),
                    ]
                    .spacing(10)
                ]
                .spacing(20)
                .align_y(Alignment::Center);

                content = content.push(header);

                // Chart type selector (only in chart mode)
                if current_tab.view_mode == ViewMode::Chart {
                    let chart_selector = row![
                        text("Chart Type:").size(16).color(Color::BLACK),
                        pick_list(
                            ChartType::all(),
                            Some(current_tab.chart_type),
                            Message::ChartTypeChanged
                        )
                        .padding(10),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center);

                    content = content.push(chart_selector);
                    
                    // Column selection for chart axes
                    let column_options: Vec<String> = result.column_names.clone();
                    
                    if !column_options.is_empty() {
                        let axis_selector = row![
                            text("X Axis:").size(14).color(Color::BLACK),
                            pick_list(
                                column_options.clone(),
                                current_tab.x_axis_column.clone(),
                                Message::XAxisColumnChanged
                            )
                            .padding(8),
                            text("Y Axis:").size(14).color(Color::BLACK),
                            pick_list(
                                column_options,
                                current_tab.y_axis_column.clone(),
                                Message::YAxisColumnChanged
                            )
                            .padding(8),
                        ]
                        .spacing(15)
                        .align_y(Alignment::Center);
                        
                        content = content.push(axis_selector);
                    }
                }

                // Content based on view mode
                match current_tab.view_mode {
                    ViewMode::Table => {
                        let table_view = Self::render_table(result);
                        content = content.push(table_view);
                    }
                    ViewMode::Chart => {
                        let chart_view = Self::render_chart(current_tab, result);
                        content = content.push(chart_view);
                    }
                }
            }
        }

        scrollable(content).into()
    }

    fn theme(&self) -> Theme {
        Theme::Light
    }

    fn render_table<'a>(result: &'a QueryResult) -> Element<'a, Message> {
        let mut table_content = column![].spacing(0);

        // Header row
        let header_cells: Vec<Element<'a, Message>> = result
            .column_names
            .iter()
            .map(|name| {
                container(text(name).size(14).color(Color::BLACK))
                    .padding(10)
                    .width(Length::Fill)
                    .style(|_theme: &Theme| {
                        container::Style {
                            background: Some(iced::Background::Color(Color::from_rgb(0.9, 0.9, 0.9))),
                            border: iced::Border {
                                color: Color::from_rgb(0.7, 0.7, 0.7),
                                width: 1.0,
                                radius: 0.0.into(),
                            },
                            ..Default::default()
                        }
                    })
                    .into()
            })
            .collect();

        let header_row = row(header_cells).spacing(0);
        table_content = table_content.push(header_row);

        // Data rows
        for (idx, row_data) in result.rows.iter().enumerate() {
            let cells: Vec<Element<'a, Message>> = row_data
                .iter()
                .map(|cell| {
                    let bg_color = if idx % 2 == 0 {
                        Color::WHITE
                    } else {
                        Color::from_rgb(0.98, 0.98, 0.98)
                    };

                    container(text(cell).size(14).color(Color::BLACK))
                        .padding(8)
                        .width(Length::Fill)
                        .style(move |_theme: &Theme| {
                            container::Style {
                                background: Some(iced::Background::Color(bg_color)),
                                border: iced::Border {
                                    color: Color::from_rgb(0.85, 0.85, 0.85),
                                    width: 1.0,
                                    radius: 0.0.into(),
                                },
                                ..Default::default()
                            }
                        })
                        .into()
                })
                .collect();

            let data_row = row(cells).spacing(0);
            table_content = table_content.push(data_row);
        }

        scrollable(table_content).into()
    }

    fn render_chart<'a>(current_tab: &'a QueryTab, result: &'a QueryResult) -> Element<'a, Message> {
        let chart_canvas = canvas(ChartCanvas {
            result: result.clone(),
            chart_type: current_tab.chart_type,
            x_axis_column: current_tab.x_axis_column.clone(),
            y_axis_column: current_tab.y_axis_column.clone(),
        })
        .width(Length::Fill)
        .height(Length::Fixed(450.0));

        let chart_info_text = if let (Some(x), Some(y)) = (&current_tab.x_axis_column, &current_tab.y_axis_column) {
            format!("Plotting: X={}, Y={}", x, y)
        } else {
            "Select columns for X and Y axes".to_string()
        };

        let chart_container = column![
            text(format!("Chart Type: {:?}", current_tab.chart_type))
                .size(16)
                .color(Color::BLACK),
            text(chart_info_text)
                .size(14)
                .color(Color::BLACK),
            chart_canvas
        ]
        .spacing(10)
        .padding(20);

        container(chart_container)
            .padding(20)
            .width(Length::Fill)
            .style(|_theme: &Theme| {
                container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(0.95, 0.95, 0.95))),
                    border: iced::Border {
                        color: Color::from_rgb(0.7, 0.7, 0.7),
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }
            })
            .into()
    }
}

#[cfg(feature = "gui")]
#[derive(Clone)]
struct ChartCanvas {
    result: QueryResult,
    chart_type: ChartType,
    x_axis_column: Option<String>,
    y_axis_column: Option<String>,
}

#[cfg(feature = "gui")]
impl canvas::Program<Message> for ChartCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Fill background
        let background = canvas::Path::rectangle(Point::ORIGIN, bounds.size());
        frame.fill(&background, Color::WHITE);

        // Get column indices
        let x_col_idx = self.x_axis_column.as_ref().and_then(|col_name| {
            self.result.column_names.iter().position(|c| c == col_name)
        });

        let y_col_idx = self.y_axis_column.as_ref().and_then(|col_name| {
            self.result.column_names.iter().position(|c| c == col_name)
        });

        if x_col_idx.is_none() || y_col_idx.is_none() || self.result.rows.is_empty() {
            return vec![frame.into_geometry()];
        }

        let x_idx = x_col_idx.unwrap();
        let y_idx = y_col_idx.unwrap();

        // Extract values
        let x_labels: Vec<String> = self.result.rows
            .iter()
            .filter_map(|row| row.get(x_idx).cloned())
            .collect();

        let y_values: Vec<f64> = self.result.rows
            .iter()
            .filter_map(|row| {
                row.get(y_idx).and_then(|v| v.parse::<f64>().ok())
            })
            .collect();

        if y_values.is_empty() || x_labels.is_empty() {
            return vec![frame.into_geometry()];
        }

        let max_value = y_values.iter().fold(0.0f64, |a, &b| a.max(b));
        let min_value = y_values.iter().fold(max_value, |a, &b| a.min(b));
        let value_range = (max_value - min_value).max(1.0);

        let y_label_width = if let Some(y_col) = &self.y_axis_column {
            (y_col.len() as f32 * 8.0 + 20.0).max(70.0)
        } else {
            50.0
        };
        
        let left_margin = y_label_width + 30.0;
        let right_margin = 40.0;
        let top_margin = 30.0;
        let bottom_margin = 80.0;
        
        let chart_height = bounds.height - top_margin - bottom_margin;
        let chart_width = bounds.width - left_margin - right_margin;
        let bar_width = (chart_width / y_values.len() as f32).min(60.0);
        let spacing = bar_width * 0.2;

        // Draw grid
        let num_horizontal_lines = 5;
        for i in 0..=num_horizontal_lines {
            let y = top_margin + (chart_height / num_horizontal_lines as f32) * i as f32;
            let grid_line = canvas::Path::line(
                Point::new(left_margin, y),
                Point::new(chart_width + left_margin, y),
            );
            frame.stroke(
                &grid_line,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.85, 0.85, 0.85))
                    .with_width(1.0),
            );
        }

        // Draw chart
        match self.chart_type {
            ChartType::Bar => {
                for (i, &value) in y_values.iter().enumerate() {
                    let normalized_height = ((value - min_value) / value_range) as f32 * chart_height;
                    let x = left_margin + i as f32 * bar_width;
                    let y = chart_height + top_margin - normalized_height;

                    let bar = canvas::Path::rectangle(
                        Point::new(x + spacing, y),
                        Size::new(bar_width - spacing * 2.0, normalized_height),
                    );
                    frame.fill(&bar, Color::from_rgb(0.3, 0.6, 0.9));
                }
            }
            ChartType::Line => {
                let mut path_builder = canvas::path::Builder::new();
                
                for (i, &value) in y_values.iter().enumerate() {
                    let normalized_height = ((value - min_value) / value_range) as f32 * chart_height;
                    let x = left_margin + i as f32 * bar_width + bar_width / 2.0;
                    let y = chart_height + top_margin - normalized_height;

                    if i == 0 {
                        path_builder.move_to(Point::new(x, y));
                    } else {
                        path_builder.line_to(Point::new(x, y));
                    }

                    let point = canvas::Path::circle(Point::new(x, y), 4.0);
                    frame.fill(&point, Color::from_rgb(0.3, 0.6, 0.9));
                }

                let line_path = path_builder.build();
                frame.stroke(
                    &line_path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.3, 0.6, 0.9))
                        .with_width(2.0),
                );
            }
            ChartType::Area => {
                let mut path_builder = canvas::path::Builder::new();
                
                let start_x = left_margin;
                let baseline_y = chart_height + top_margin;
                path_builder.move_to(Point::new(start_x, baseline_y));

                if let Some(&first_value) = y_values.first() {
                    let normalized_height = ((first_value - min_value) / value_range) as f32 * chart_height;
                    let y = chart_height + top_margin - normalized_height;
                    path_builder.line_to(Point::new(start_x, y));
                }

                for (i, &value) in y_values.iter().enumerate() {
                    let normalized_height = ((value - min_value) / value_range) as f32 * chart_height;
                    let x = left_margin + i as f32 * bar_width + bar_width / 2.0;
                    let y = chart_height + top_margin - normalized_height;
                    path_builder.line_to(Point::new(x, y));
                }

                let end_x = left_margin + (y_values.len() - 1) as f32 * bar_width + bar_width / 2.0;
                path_builder.line_to(Point::new(end_x, baseline_y));
                path_builder.line_to(Point::new(start_x, baseline_y));

                let area_path = path_builder.build();
                frame.fill(&area_path, Color::from_rgba(0.3, 0.6, 0.9, 0.3));
                
                let mut outline_builder = canvas::path::Builder::new();
                for (i, &value) in y_values.iter().enumerate() {
                    let normalized_height = ((value - min_value) / value_range) as f32 * chart_height;
                    let x = left_margin + i as f32 * bar_width + bar_width / 2.0;
                    let y = chart_height + top_margin - normalized_height;
                    
                    if i == 0 {
                        outline_builder.move_to(Point::new(x, y));
                    } else {
                        outline_builder.line_to(Point::new(x, y));
                    }
                }
                let outline_path = outline_builder.build();
                frame.stroke(
                    &outline_path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.3, 0.6, 0.9))
                        .with_width(2.0),
                );
            }
            ChartType::Scatter => {
                for (i, &value) in y_values.iter().enumerate() {
                    let normalized_height = ((value - min_value) / value_range) as f32 * chart_height;
                    let x = left_margin + i as f32 * bar_width + bar_width / 2.0;
                    let y = chart_height + top_margin - normalized_height;

                    let point = canvas::Path::circle(Point::new(x, y), 5.0);
                    frame.fill(&point, Color::from_rgb(0.3, 0.6, 0.9));
                }
            }
        }

        // Draw axes
        let y_axis = canvas::Path::line(
            Point::new(left_margin, top_margin),
            Point::new(left_margin, chart_height + top_margin),
        );
        frame.stroke(
            &y_axis,
            canvas::Stroke::default()
                .with_color(Color::BLACK)
                .with_width(2.0),
        );

        let x_axis = canvas::Path::line(
            Point::new(left_margin, chart_height + top_margin),
            Point::new(chart_width + left_margin, chart_height + top_margin),
        );
        frame.stroke(
            &x_axis,
            canvas::Stroke::default()
                .with_color(Color::BLACK)
                .with_width(2.0),
        );

        // Y-axis labels
        let num_y_ticks = 5;
        for i in 0..=num_y_ticks {
            let y = chart_height + top_margin - (chart_height / num_y_ticks as f32) * i as f32;
            let value = min_value + (value_range / num_y_ticks as f64) * i as f64;
            
            let tick = canvas::Path::line(
                Point::new(left_margin - 5.0, y),
                Point::new(left_margin, y),
            );
            frame.stroke(
                &tick,
                canvas::Stroke::default()
                    .with_color(Color::BLACK)
                    .with_width(1.5),
            );

            frame.fill_text(canvas::Text {
                content: format!("{:.1}", value),
                position: Point::new(left_margin - 10.0, y - 7.0),
                color: Color::BLACK,
                size: 11.0.into(),
                font: Font::default(),
                horizontal_alignment: iced::alignment::Horizontal::Right,
                vertical_alignment: iced::alignment::Vertical::Top,
                ..canvas::Text::default()
            });
        }

        // Y-axis title
        if let Some(y_col) = &self.y_axis_column {
            frame.fill_text(canvas::Text {
                content: y_col.clone(),
                position: Point::new(10.0, top_margin + chart_height / 2.0),
                color: Color::BLACK,
                size: 13.0.into(),
                font: Font::default(),
                horizontal_alignment: iced::alignment::Horizontal::Left,
                vertical_alignment: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }

        // X-axis labels
        for (i, label) in x_labels.iter().enumerate().take(y_values.len()) {
            let x = left_margin + i as f32 * bar_width + bar_width / 2.0;
            
            let tick = canvas::Path::line(
                Point::new(x, chart_height + top_margin),
                Point::new(x, chart_height + top_margin + 5.0),
            );
            frame.stroke(
                &tick,
                canvas::Stroke::default()
                    .with_color(Color::BLACK)
                    .with_width(1.5),
            );

            let display_label = if label.len() > 8 {
                format!("{}...", &label[..5])
            } else {
                label.clone()
            };

            frame.fill_text(canvas::Text {
                content: display_label,
                position: Point::new(x, chart_height + top_margin + 8.0),
                color: Color::BLACK,
                size: 10.0.into(),
                font: Font::default(),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Top,
                ..canvas::Text::default()
            });
        }

        // X-axis title
        if let Some(x_col) = &self.x_axis_column {
            frame.fill_text(canvas::Text {
                content: x_col.clone(),
                position: Point::new(left_margin + chart_width / 2.0, chart_height + top_margin + 50.0),
                color: Color::BLACK,
                size: 14.0.into(),
                font: Font::default(),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Top,
                ..canvas::Text::default()
            });
        }

        vec![frame.into_geometry()]
    }
}

// ============================================================================
// VizBuilder and ChartConfig
// ============================================================================

#[cfg(feature = "gui")]
#[derive(Debug, Clone)]
pub struct ChartConfig {
    pub chart_type: Option<ChartType>,
    pub x_axis_column: Option<String>,
    pub y_axis_column: Option<String>,
}

#[cfg(feature = "gui")]
impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            chart_type: None,
            x_axis_column: None,
            y_axis_column: None,
        }
    }
}

#[cfg(feature = "gui")]
static CONFIGS_STORAGE: std::sync::OnceLock<Arc<Mutex<Vec<ChartConfig>>>> = std::sync::OnceLock::new();

#[cfg(feature = "gui")]
#[derive(Clone)]
pub struct VizBuilder {
    queries: Vec<String>,
    configs: Vec<ChartConfig>,
}

#[cfg(feature = "gui")]
impl VizBuilder {
    pub fn new() -> Self {
        Self {
            queries: Vec::new(),
            configs: Vec::new(),
        }
    }

    pub fn add_query(mut self, sql: &str) -> Self {
        let split_queries: Vec<&str> = sql
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for query in split_queries {
            self.queries.push(query.to_string());
            self.configs.push(ChartConfig::default());
        }
        self
    }

    pub fn with_chart(mut self, chart_type: ChartType, x_column: &str, y_column: &str) -> Self {
        if let Some(last_config) = self.configs.last_mut() {
            *last_config = ChartConfig {
                chart_type: Some(chart_type),
                x_axis_column: Some(x_column.to_string()),
                y_axis_column: Some(y_column.to_string()),
            };
        }
        self
    }

    pub fn launch(self) -> Result<()> {
        if self.queries.is_empty() {
            return Err(anyhow::anyhow!("No queries provided"));
        }

        let combined_sql = self.queries.join("; ");
        let configs_arc = Arc::new(Mutex::new(self.configs));
        let _ = CONFIGS_STORAGE.set(configs_arc);
        let sql_arc = Arc::new(Mutex::new(combined_sql));
        let _ = SQL_STORAGE.set(sql_arc);

        iced::application("SQL2VIZ - Query Viewer", AppState::update, AppState::view)
            .theme(AppState::theme)
            .run_with(|| {
                let sql = SQL_STORAGE
                    .get()
                    .and_then(|storage| storage.lock().ok())
                    .map(|guard| guard.clone())
                    .unwrap_or_default();
                (AppState::new(sql), Task::none())
            })
            .map_err(|e| anyhow::anyhow!("Failed to launch GUI: {}", e))
    }
}

// ============================================================================
// ネイティブ専用: クエリ実行ヘルパー
// ============================================================================

#[cfg(all(not(target_arch = "wasm32"), feature = "duckdb", feature = "gui"))]
fn execute_queries_with_configs(sql: &str) -> Vec<QueryTab> {
    let mut tabs = Vec::new();

    if sql.trim().is_empty() {
        tabs.push(QueryTab {
            name: "Error".to_string(),
            result: None,
            error: Some("No query provided".to_string()),
            view_mode: ViewMode::Table,
            chart_type: ChartType::Bar,
            x_axis_column: None,
            y_axis_column: None,
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
                view_mode: ViewMode::Table,
                chart_type: ChartType::Bar,
                x_axis_column: None,
                y_axis_column: None,
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
            view_mode: ViewMode::Table,
            chart_type: ChartType::Bar,
            x_axis_column: None,
            y_axis_column: None,
        });
        return tabs;
    }

    // 保存された設定を取得
    let configs = CONFIGS_STORAGE
        .get()
        .and_then(|storage| storage.lock().ok())
        .map(|guard| guard.clone())
        .unwrap_or_default();

    for (idx, query) in queries.iter().enumerate() {
        let tab_name = format!("Query {}", idx + 1);
        
        // 対応する設定を取得（なければデフォルト）
        let config = configs.get(idx).cloned().unwrap_or_default();

        match duck_table.query_raw(query) {
            Ok(result) => {
                // チャート設定がある場合は自動的にChartモードに
                let view_mode = if config.chart_type.is_some() {
                    ViewMode::Chart
                } else {
                    ViewMode::Table
                };

                // X軸・Y軸の初期値を設定
                let x_col = config.x_axis_column.or_else(|| result.column_names.first().cloned());
                let y_col = config.y_axis_column.or_else(|| {
                    result.column_names.get(1).or_else(|| result.column_names.first()).cloned()
                });

                tabs.push(QueryTab {
                    name: tab_name,
                    result: Some(result),
                    error: None,
                    view_mode,
                    chart_type: config.chart_type.unwrap_or(ChartType::Bar),
                    x_axis_column: x_col,
                    y_axis_column: y_col,
                });
            }
            Err(e) => {
                tabs.push(QueryTab {
                    name: tab_name,
                    result: None,
                    error: Some(e.to_string()),
                    view_mode: ViewMode::Table,
                    chart_type: ChartType::Bar,
                    x_axis_column: None,
                    y_axis_column: None,
                });
            }
        }
    }

    tabs
}

// ============================================================================
// ネイティブ専用: 公開API
// ============================================================================

/// Launch SQL2VIZ GUI with custom SQL query (ネイティブ専用)
#[cfg(all(not(target_arch = "wasm32"), feature = "gui", feature = "duckdb"))]
pub fn vizcreate(sql: String) -> Result<()> {
    if sql.trim().is_empty() {
        return Err(anyhow::anyhow!("No SQL query provided"));
    }

    let sql_arc = Arc::new(Mutex::new(sql));
    let _ = SQL_STORAGE.set(sql_arc);

    iced::application("SQL2VIZ - Query Viewer", AppState::update, AppState::view)
        .theme(AppState::theme)
        .run_with(|| {
            let sql = SQL_STORAGE
                .get()
                .and_then(|storage| storage.lock().ok())
                .map(|guard| guard.clone())
                .unwrap_or_default();

            (AppState::new(sql), Task::none())
        })
        .map_err(|e| anyhow::anyhow!("Failed to launch GUI: {}", e))
}

/// Launch SQL2VIZ GUI with an example query (ネイティブ専用)
#[cfg(all(not(target_arch = "wasm32"), feature = "gui", feature = "duckdb"))]
pub fn vizcreate_example() -> Result<()> {
    let example = "SELECT 'Example' as name, 42 as value";
    vizcreate(example.to_string())
}

// 後方互換性のため古い関数名も残す（非推奨）
#[cfg(all(not(target_arch = "wasm32"), feature = "gui", feature = "duckdb"))]
#[deprecated(since = "0.2.0", note = "Use `vizcreate` instead")]
pub fn launch_simple_gui(sql: String) -> Result<()> {
    vizcreate(sql)
}

#[cfg(all(not(target_arch = "wasm32"), feature = "gui", feature = "duckdb"))]
#[deprecated(since = "0.2.0", note = "Use `vizcreate_example` instead")]
pub fn launch_gui() -> Result<()> {
    vizcreate_example()
}

// ============================================================================
// Python bindings
// ============================================================================

#[cfg(feature = "python")]
mod python_bindings;