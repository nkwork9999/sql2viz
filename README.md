````markdown
# sql2viz

Transform SQL queries into visualizations using DuckDB and Iced.

## Installation

```toml
[dependencies]
sql2viz = { version = "0.1", features = ["gui"] }
```
````

## Usage

### Basic Query

```rust
use sql2viz::vizcreate;

fn main() {
    let query = "SELECT 'A' as x, 10 as y UNION ALL SELECT 'B', 20";
    vizcreate(query.to_string()).unwrap();
}
```

### CSV File

```rust
use sql2viz::vizcreate;

fn main() {
    let query = "SELECT * FROM read_csv_auto('data.csv')";
    vizcreate(query.to_string()).unwrap();
}
```

## Features

- SQL query execution with DuckDB
- Interactive charts (Bar, Line, Area, Scatter)
- Table view
- Column selection for chart axes
- Direct CSV file reading

## crate.io

https://crates.io/crates/sql2viz

## License

MIT OR Apache-2.0

```

```
