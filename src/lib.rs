use anyhow::Result;
use duckdb::{params, Connection, Row};
use thiserror::Error;

#[cfg(feature = "tabled-backend")]
use tabled::{builder::Builder as TabledBuilder, settings::Style};

#[cfg(feature = "comfy-backend")]
use comfy_table::{presets, Cell, ContentArrangement, Table as ComfyTable};

#[cfg(feature = "colored")]
use colored::Colorize;

/// Custom error types for the library
#[derive(Error, Debug)]
pub enum DuckTableError {
    #[error("DuckDB error: {0}")]
    DatabaseError(#[from] duckdb::Error),

    #[error("Table formatting error: {0}")]
    FormattingError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("No backend selected. Enable either 'tabled-backend' or 'comfy-backend' feature")]
    NoBackendError,
}

/// Table style presets
#[derive(Debug, Clone, Copy)]
pub enum TableStyle {
    Ascii,
    Unicode,
    Rounded,
    Markdown,
    Minimal,
    Blank,
    Custom,
}

/// Display backend selection
#[derive(Debug, Clone, Copy)]
pub enum DisplayBackend {
    #[cfg(feature = "tabled-backend")]
    Tabled,
    #[cfg(feature = "comfy-backend")]
    Comfy,
    Auto,
}

/// Configuration for table display
#[derive(Debug, Clone)]
pub struct DisplayConfig {
    pub style: TableStyle,
    pub backend: DisplayBackend,
    pub max_column_width: usize,
    pub show_row_numbers: bool,
    #[cfg(feature = "colored")]
    pub colored_headers: bool,
    pub row_limit: usize,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            style: TableStyle::Unicode,
            backend: DisplayBackend::Auto,
            max_column_width: 50,
            show_row_numbers: false,
            #[cfg(feature = "colored")]
            colored_headers: true,
            row_limit: 0,
        }
    }
}

/// Main struct for executing queries and displaying results
pub struct DuckTable {
    connection: Connection,
    config: DisplayConfig,
}

impl DuckTable {
    /// Create a new DuckTable with an in-memory database
    pub fn new() -> Result<Self> {
        Ok(Self {
            connection: Connection::open_in_memory()?,
            config: DisplayConfig::default(),
        })
    }

    /// Create a new DuckTable with a file-based database
    pub fn with_file(path: &str) -> Result<Self> {
        Ok(Self {
            connection: Connection::open(path)?,
            config: DisplayConfig::default(),
        })
    }

    /// Create with existing connection
    pub fn with_connection(connection: Connection) -> Self {
        Self {
            connection,
            config: DisplayConfig::default(),
        }
    }

    /// Set display configuration
    pub fn set_config(&mut self, config: DisplayConfig) {
        self.config = config;
    }

    /// Execute a SQL query and return formatted table as string
    pub fn query(&self, sql: &str) -> Result<String> {
        let mut stmt = self.connection.prepare(sql)?;
        let mut rows = stmt.query(params![])?;
        
        // Use rows.as_ref() to access the statement and get column information
        // This is the correct way to get metadata while iterating
        let stmt_ref = rows.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Failed to get statement reference")
        })?;
        
        let column_count = stmt_ref.column_count();
        let mut column_names = Vec::new();
        
        for i in 0..column_count {
            let name = stmt_ref.column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col_{}", i));
            column_names.push(name);
        }
        
        if column_count == 0 {
            return Ok("(empty result)".to_string());
        }
        
        // Collect all data
        let mut all_rows = Vec::new();
        
        while let Some(row) = rows.next()? {
            let mut row_values = Vec::new();
            
            for i in 0..column_count {
                let value = self.extract_value_from_row(row, i);
                row_values.push(value);
            }
            
            all_rows.push(row_values);
            
            if self.config.row_limit > 0 && all_rows.len() >= self.config.row_limit {
                break;
            }
        }

        self.format_table(&column_names, &all_rows)
    }

    /// Extract value from a row safely
    fn extract_value_from_row(&self, row: &Row, index: usize) -> String {
        use duckdb::types::ValueRef;
        
        match row.get_ref(index) {
            Ok(value_ref) => {
                match value_ref {
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
                    },
                    ValueRef::Double(d) => {
                        if d.fract() == 0.0 && d.abs() < 1e10 {
                            format!("{:.0}", d)
                        } else {
                            format!("{:.2}", d)
                        }
                    },
                    ValueRef::Decimal(decimal) => {
                        // Convert decimal to f64 for display
                        format!("{:.2}", decimal.to_string().parse::<f64>().unwrap_or(0.0))
                    },
                    ValueRef::Text(bytes) => {
                        String::from_utf8_lossy(bytes).to_string()
                    },
                    ValueRef::Blob(bytes) => {
                        if let Ok(s) = std::str::from_utf8(bytes) {
                            s.to_string()
                        } else {
                            format!("<blob {} bytes>", bytes.len())
                        }
                    },
                    ValueRef::Date32(days) => {
                        format!("Date({})", days)
                    },
                    ValueRef::Timestamp(_, micros) => {
                        format!("Timestamp({})", micros)
                    },
                    ValueRef::Time64(_, nanos) => {
                        format!("Time({})", nanos)
                    },
                    // Handle any other variants
                    _ => format!("{:?}", value_ref)
                }
            }
            Err(_) => {
                // Fallback: try to get as string
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
    
    /// Format the table based on selected backend
    fn format_table(&self, headers: &[String], rows: &[Vec<String>]) -> Result<String> {
        match self.config.backend {
            #[cfg(feature = "tabled-backend")]
            DisplayBackend::Tabled => self.format_with_tabled(headers, rows),

            #[cfg(feature = "comfy-backend")]
            DisplayBackend::Comfy => self.format_with_comfy(headers, rows),

            DisplayBackend::Auto => {
                #[cfg(feature = "tabled-backend")]
                return self.format_with_tabled(headers, rows);

                #[cfg(all(not(feature = "tabled-backend"), feature = "comfy-backend"))]
                return self.format_with_comfy(headers, rows);

                #[cfg(all(not(feature = "tabled-backend"), not(feature = "comfy-backend")))]
                Err(DuckTableError::NoBackendError.into())
            }
        }
    }

    #[cfg(feature = "tabled-backend")]
    fn format_with_tabled(&self, headers: &[String], rows: &[Vec<String>]) -> Result<String> {
        let mut builder = TabledBuilder::new();

        let mut header_row = if self.config.show_row_numbers {
            let mut h = vec!["#".to_string()];
            h.extend(headers.iter().cloned());
            h
        } else {
            headers.to_vec()
        };

        #[cfg(feature = "colored")]
        if self.config.colored_headers {
            header_row = header_row.iter().map(|h| h.bold().blue().to_string()).collect();
        }

        builder.push_record(header_row);

        for (idx, row) in rows.iter().enumerate() {
            let mut row_data = if self.config.show_row_numbers {
                vec![(idx + 1).to_string()]
            } else {
                vec![]
            };

            for value in row {
                let truncated = if self.config.max_column_width > 0 && value.len() > self.config.max_column_width {
                    format!("{}...", &value[..self.config.max_column_width.saturating_sub(3)])
                } else {
                    value.clone()
                };
                row_data.push(truncated);
            }

            builder.push_record(row_data);
        }

        let mut table = builder.build();
        
        match self.config.style {
            TableStyle::Ascii => table.with(Style::ascii()),
            TableStyle::Unicode => table.with(Style::modern()),
            TableStyle::Rounded => table.with(Style::modern_rounded()),
            TableStyle::Markdown => table.with(Style::markdown()),
            TableStyle::Minimal => table.with(Style::extended()),
            TableStyle::Blank => table.with(Style::blank()),
            TableStyle::Custom => table.with(Style::modern()),
        };

        Ok(table.to_string())
    }

    #[cfg(feature = "comfy-backend")]
    fn format_with_comfy(&self, headers: &[String], rows: &[Vec<String>]) -> Result<String> {
        let mut table = ComfyTable::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        if self.config.max_column_width > 0 && !headers.is_empty() {
             table.set_width((self.config.max_column_width * headers.len()) as u16);
        }

        let mut header_cells = Vec::new();
        if self.config.show_row_numbers {
            header_cells.push(Cell::new("#"));
        }
        for header in headers {
            #[cfg(feature = "colored")]
            let header_text = if self.config.colored_headers {
                header.bold().blue().to_string()
            } else {
                header.clone()
            };
            #[cfg(not(feature = "colored"))]
            let header_text = header.clone();

            header_cells.push(Cell::new(header_text));
        }
        table.set_header(header_cells);

        for (idx, row) in rows.iter().enumerate() {
            let mut row_cells = Vec::new();
            if self.config.show_row_numbers {
                row_cells.push(Cell::new(idx + 1));
            }
            for value in row {
                row_cells.push(Cell::new(value));
            }
            table.add_row(row_cells);
        }
        
        table.load_preset(self.get_comfy_preset());

        Ok(table.to_string())
    }

    #[cfg(feature = "comfy-backend")]
    fn get_comfy_preset(&self) -> &str {
        match self.config.style {
            TableStyle::Ascii => presets::ASCII_FULL,
            TableStyle::Unicode => presets::UTF8_FULL,
            TableStyle::Rounded => presets::UTF8_ROUND_CORNERS,
            TableStyle::Markdown => presets::ASCII_MARKDOWN,
            TableStyle::Minimal => presets::UTF8_HORIZONTAL_ONLY,
            TableStyle::Blank => presets::NOTHING,
            TableStyle::Custom => presets::UTF8_FULL,
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

/// Builder pattern for configuring DuckTable
#[derive(Default)]
pub struct DuckTableBuilder {
    path: Option<String>,
    config: DisplayConfig,
}

impl DuckTableBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn style(mut self, style: TableStyle) -> Self {
        self.config.style = style;
        self
    }

    pub fn backend(mut self, backend: DisplayBackend) -> Self {
        self.config.backend = backend;
        self
    }

    pub fn max_column_width(mut self, width: usize) -> Self {
        self.config.max_column_width = width;
        self
    }

    pub fn show_row_numbers(mut self, show: bool) -> Self {
        self.config.show_row_numbers = show;
        self
    }

    #[cfg(feature = "colored")]
    pub fn colored_headers(mut self, colored: bool) -> Self {
        self.config.colored_headers = colored;
        self
    }

    pub fn row_limit(mut self, limit: usize) -> Self {
        self.config.row_limit = limit;
        self
    }

    pub fn build(self) -> Result<DuckTable> {
        let connection = match self.path {
            Some(path) => Connection::open(path)?,
            None => Connection::open_in_memory()?,
        };

        Ok(DuckTable {
            connection,
            config: self.config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_query() {
        let duck = DuckTable::new().unwrap();
        let result = duck.query("SELECT 42 as answer, 'hello' as greeting").unwrap();
        assert!(result.contains("42"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_builder_pattern() {
        let duck = DuckTableBuilder::new()
            .style(TableStyle::Ascii)
            .show_row_numbers(true)
            .build()
            .unwrap();

        let result = duck.query("SELECT 1 as num").unwrap();
        assert!(result.contains("1"));
    }
}