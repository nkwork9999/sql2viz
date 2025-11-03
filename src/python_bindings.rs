#![cfg(feature = "python")]

use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use crate::DuckTable;

/// Python wrapper for DuckTable
#[pyclass]
pub struct PyDuckTable {
    inner: DuckTable,
}

#[pymethods]
impl PyDuckTable {
    #[new]
    fn new() -> PyResult<Self> {
        DuckTable::new()
            .map(|inner| Self { inner })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[staticmethod]
    fn with_file(path: &str) -> PyResult<Self> {
        DuckTable::with_file(path)
            .map(|inner| Self { inner })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn query(&self, sql: &str) -> PyResult<String> {
        self.inner.query(sql)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn query_raw(&self, sql: &str) -> PyResult<PyQueryResult> {
        self.inner.query_raw(sql)
            .map(|r| PyQueryResult {
                column_names: r.column_names,
                rows: r.rows,
            })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

/// Python wrapper for QueryResult
#[pyclass]
#[derive(Clone)]
pub struct PyQueryResult {
    #[pyo3(get)]
    pub column_names: Vec<String>,
    #[pyo3(get)]
    pub rows: Vec<Vec<String>>,
}

#[pymethods]
impl PyQueryResult {
    fn __len__(&self) -> usize {
        self.rows.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "QueryResult(columns={}, rows={})",
            self.column_names.len(),
            self.rows.len()
        )
    }
}

/// Python wrapper for VizBuilder
#[cfg(feature = "gui")]
#[pyclass]
pub struct PyVizBuilder {
    inner: crate::VizBuilder,
}

#[cfg(feature = "gui")]
#[pymethods]
impl PyVizBuilder {
    #[new]
    fn new() -> Self {
        Self {
            inner: crate::VizBuilder::new(),
        }
    }

    /// クエリを追加
    /// 
    /// メソッドチェーンのために selfを返す
    fn add_query(&mut self, sql: &str) {
        self.inner = self.inner.clone().add_query(sql);
    }

    /// 直前に追加したクエリにチャート設定を適用
    /// 
    /// Parameters:
    /// -----------
    /// chart_type : str
    ///     "Bar", "Line", "Area", "Scatter" のいずれか
    /// x_column : str
    ///     X軸に使用する列名
    /// y_column : str
    ///     Y軸に使用する列名
    fn with_chart(
        &mut self,
        chart_type: &str,
        x_column: &str,
        y_column: &str,
    ) -> PyResult<()> {
        let chart_type = match chart_type {
            "Bar" => crate::ChartType::Bar,
            "Line" => crate::ChartType::Line,
            "Area" => crate::ChartType::Area,
            "Scatter" => crate::ChartType::Scatter,
            _ => {
                return Err(PyRuntimeError::new_err(format!(
                    "Invalid chart type: '{}'. Must be one of: 'Bar', 'Line', 'Area', 'Scatter'",
                    chart_type
                )))
            }
        };

        self.inner = self.inner.clone().with_chart(chart_type, x_column, y_column);
        Ok(())
    }

    /// GUIを起動
    fn launch(&self) -> PyResult<()> {
        self.inner
            .clone()
            .launch()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        "VizBuilder()".to_string()
    }
}

/// Launch visualization GUI
#[pyfunction]
fn vizcreate(sql: String) -> PyResult<()> {
    #[cfg(feature = "gui")]
    {
        crate::vizcreate(sql)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[cfg(not(feature = "gui"))]
    {
        Err(PyRuntimeError::new_err(
            "GUI feature not enabled. Build with: maturin develop --features gui"
        ))
    }
}

/// Python module definition
#[pymodule]
fn sql2viz(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDuckTable>()?;
    m.add_class::<PyQueryResult>()?;

    #[cfg(feature = "gui")]
    m.add_class::<PyVizBuilder>()?;

    m.add_function(wrap_pyfunction!(vizcreate, m)?)?;

    m.add("__version__", "0.2.0")?;

    Ok(())
}