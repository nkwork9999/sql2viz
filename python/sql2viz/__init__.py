from .sql2viz import vizcreate, PyDuckTable, PyQueryResult, PyVizBuilder
import duckdb
from duckdb import DuckDBPyRelation
import tempfile

__all__ = ['vizcreate', 'PyDuckTable', 'PyQueryResult', 'VizBuilder']
__version__ = '0.2.0'


def _viz_method(self, chart_type=None, x_column=None, y_column=None):
    """
    DuckDBPyRelationに可視化機能を追加
    
    Parameters:
    -----------
    chart_type : str, optional
        チャートの種類: "Bar", "Line", "Area", "Scatter"
    x_column : str, optional
        X軸に使用する列名
    y_column : str, optional
        Y軸に使用する列名
    
    Usage:
    ------
    # シンプルな使い方（デフォルト設定）
    duckdb.sql("SELECT * FROM sales").viz()
    
    # チャート設定を指定
    duckdb.sql("SELECT month, revenue FROM sales").viz(
        chart_type="Line",
        x_column="month",
        y_column="revenue"
    )
    
    Returns:
    --------
    self : DuckDBPyRelation
        メソッドチェーン用に自身を返す
    """
    df = self.df()
    temp_file = tempfile.NamedTemporaryFile(suffix='.parquet', delete=False)
    df.to_parquet(temp_file.name)
    query = f"SELECT * FROM '{temp_file.name}'"
    
    # チャート設定が指定されている場合
    if chart_type is not None or x_column is not None or y_column is not None:
        # 全て指定されているかチェック
        if chart_type is not None and x_column is not None and y_column is not None:
            builder = PyVizBuilder()
            builder.add_query(query)
            builder.with_chart(chart_type, x_column, y_column)
            builder.launch()
        else:
            # 一部だけ指定された場合は警告を出してデフォルトで起動
            import warnings
            warnings.warn(
                "chart_type, x_column, y_column must all be specified together. "
                "Opening with default settings.",
                UserWarning
            )
            vizcreate(query)
    else:
        # 指定なしの場合は従来通り
        vizcreate(query)
    
    return self


# DuckDBPyRelationに.viz()メソッドを追加
DuckDBPyRelation.viz = _viz_method


# ビルダーをエクスポート（より Pythonic な名前）
VizBuilder = PyVizBuilder