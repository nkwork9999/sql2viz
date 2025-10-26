from .sql2viz import vizcreate, PyDuckTable, PyQueryResult
import duckdb
from duckdb import DuckDBPyRelation
import tempfile

__all__ = ['vizcreate', 'PyDuckTable', 'PyQueryResult']
__version__ = '0.2.0'


def _viz_method(self):
    df = self.df()
    temp_file = tempfile.NamedTemporaryFile(suffix='.parquet', delete=False)
    df.to_parquet(temp_file.name)
    query = f"SELECT * FROM '{temp_file.name}'"
    vizcreate(query)
    return self


DuckDBPyRelation.viz = _viz_method