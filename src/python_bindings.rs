use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use crate::{DuckTable, QueryResult};

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
    m.add_function(wrap_pyfunction!(vizcreate, m)?)?;
    
    m.add("__version__", "0.2.0")?;
    
    Ok(())
}