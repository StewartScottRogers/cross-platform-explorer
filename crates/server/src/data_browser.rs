//! Structured-data browser reader (CPE-847, epic CPE-721): schema + **paged rows** for the data files
//! the CSV/JSON preview can't open — SQLite databases, Parquet files, and Excel/ODS workbooks.
//!
//! Where [`crate::data_preview`] produces a one-shot text *summary*, this exposes the data as a grid the
//! frontend can page through: [`sources`] lists the tables/views (SQLite) or sheets (Excel), and [`page`]
//! returns a window of rows (`offset`/`limit`) with typed [`Column`]s. Cells are stringified for a uniform
//! grid. Reuses the crates already in `cpe-server` (rusqlite / calamine / parquet) — **no new dependency**.
//! Read-only throughout; the read-only SQL console is CPE-848, the grid UI is CPE-849.

use std::path::Path;

use serde::Serialize;

/// One column of a result grid: its name and a best-effort type label (the declared SQLite type, the
/// Parquet physical type; empty for spreadsheet headers, which carry no type).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
}

/// A window of rows from a data source: the columns, the row window (each cell stringified), and the
/// total row count when known (so the UI can size a scrollbar without loading everything).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Page {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Sqlite,
    Parquet,
    Spreadsheet,
}

fn detect(path: &str) -> Option<Format> {
    let ext = Path::new(path).extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "db" | "sqlite" | "sqlite3" => Some(Format::Sqlite),
        "parquet" => Some(Format::Parquet),
        "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" => Some(Format::Spreadsheet),
        _ => None,
    }
}

/// List the browsable sources in a data file: SQLite tables/views (name-sorted), Excel/ODS sheet names.
/// Parquet is a single source, so it returns an empty list (call [`page`] with `source = ""`).
pub fn sources(path: &str) -> Result<Vec<String>, String> {
    match detect(path).ok_or_else(|| "unsupported data file".to_string())? {
        Format::Sqlite => sqlite_sources(path),
        Format::Parquet => Ok(Vec::new()),
        Format::Spreadsheet => spreadsheet_sources(path),
    }
}

/// Read a window of rows (`offset`/`limit`) from `source` in the file — a table/view (SQLite), a sheet
/// (Excel/ODS), or the whole file (Parquet, `source` ignored). An out-of-range `offset` yields an empty
/// window, not an error.
pub fn page(path: &str, source: &str, offset: usize, limit: usize) -> Result<Page, String> {
    match detect(path).ok_or_else(|| "unsupported data file".to_string())? {
        Format::Sqlite => sqlite_page(path, source, offset, limit),
        Format::Parquet => parquet_page(path, offset, limit),
        Format::Spreadsheet => spreadsheet_page(path, source, offset, limit),
    }
}

// ---------------------------------------------------------------------------
// SQLite
// ---------------------------------------------------------------------------

fn sqlite_open(path: &str) -> Result<rusqlite::Connection, String> {
    use rusqlite::{Connection, OpenFlags};
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())
}

fn sqlite_sources(path: &str) -> Result<Vec<String>, String> {
    let conn = sqlite_open(path)?;
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type IN ('table','view') \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Double-quote + escape an identifier so a table/sheet name can't break the SQL (the names come from
/// `sqlite_master`, but quoting keeps odd names — spaces, keywords — safe).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn sqlite_page(path: &str, source: &str, offset: usize, limit: usize) -> Result<Page, String> {
    if source.is_empty() {
        return Err("a table or view name is required".to_string());
    }
    let conn = sqlite_open(path)?;
    let quoted = quote_ident(source);

    // Columns (name + declared type) from PRAGMA table_info — same order as SELECT *.
    let columns: Vec<Column> = {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({quoted})"))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Column {
                    name: r.get::<_, String>(1)?,
                    type_: r.get::<_, String>(2).unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    if columns.is_empty() {
        return Err(format!("no such table or view: {source}"));
    }

    let total = conn
        .query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |r| r.get::<_, i64>(0))
        .ok()
        .map(|c| c as u64);

    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {quoted} LIMIT ?1 OFFSET ?2"))
        .map_err(|e| e.to_string())?;
    let ncol = stmt.column_count();
    let rows = stmt
        .query_map([limit as i64, offset as i64], |r| {
            let mut cells = Vec::with_capacity(ncol);
            for i in 0..ncol {
                cells.push(r.get_ref(i).map(value_ref_to_string).unwrap_or_default());
            }
            Ok(cells)
        })
        .map_err(|e| e.to_string())?;
    let rows: Vec<Vec<String>> = rows.filter_map(|r| r.ok()).collect();

    Ok(Page { columns, rows, total })
}

fn value_ref_to_string(v: rusqlite::types::ValueRef<'_>) -> String {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => format!("<{} bytes>", b.len()),
    }
}

// ---------------------------------------------------------------------------
// Spreadsheet (Excel / ODS) — first row is treated as the header.
// ---------------------------------------------------------------------------

fn spreadsheet_sources(path: &str) -> Result<Vec<String>, String> {
    use calamine::{open_workbook_auto, Reader};
    let wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    Ok(wb.sheet_names().to_vec())
}

fn spreadsheet_page(path: &str, source: &str, offset: usize, limit: usize) -> Result<Page, String> {
    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    // Default to the first sheet when none is named.
    let sheet = if source.is_empty() {
        wb.sheet_names().first().cloned().ok_or_else(|| "workbook has no sheets".to_string())?
    } else {
        source.to_string()
    };
    let range = wb.worksheet_range(&sheet).map_err(|e| e.to_string())?;

    let mut iter = range.rows();
    let columns: Vec<Column> = match iter.next() {
        Some(header) => header
            .iter()
            .map(|c| Column { name: c.to_string(), type_: String::new() })
            .collect(),
        None => Vec::new(),
    };
    // Remaining rows are data; page over them.
    let data: Vec<Vec<String>> = iter
        .skip(offset)
        .take(limit)
        .map(|row| row.iter().map(|c| c.to_string()).collect())
        .collect();
    // Total data rows = height minus the header row (when there is one).
    let total = Some(range.height().saturating_sub(1) as u64);

    Ok(Page { columns, rows: data, total })
}

// ---------------------------------------------------------------------------
// Parquet
// ---------------------------------------------------------------------------

fn parquet_page(path: &str, offset: usize, limit: usize) -> Result<Page, String> {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = SerializedFileReader::new(file).map_err(|e| e.to_string())?;
    let fmeta = reader.metadata().file_metadata();
    let schema = fmeta.schema_descr();
    let columns: Vec<Column> = (0..schema.num_columns())
        .map(|i| {
            let c = schema.column(i);
            Column { name: c.name().to_string(), type_: format!("{:?}", c.physical_type()) }
        })
        .collect();
    let total = Some(fmeta.num_rows() as u64);

    let iter = reader.get_row_iter(None).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for item in iter.skip(offset).take(limit) {
        let row = item.map_err(|e| e.to_string())?;
        rows.push(row.get_column_iter().map(|(_, f)| f.to_string()).collect());
    }

    Ok(Page { columns, rows, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-databrowser-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sqlite_sources_and_paged_rows() {
        use rusqlite::Connection;
        let d = scratch("sqlite");
        let f = d.join("test.db");
        {
            let conn = Connection::open(&f).unwrap();
            conn.execute_batch(
                "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT);
                 CREATE VIEW v AS SELECT * FROM people;
                 INSERT INTO people (name) VALUES ('Ann'),('Bo'),('Cy'),('Di');",
            )
            .unwrap();
        }
        let p = f.to_string_lossy().to_string();
        assert_eq!(sources(&p).unwrap(), vec!["people".to_string(), "v".to_string()]);

        let page1 = page(&p, "people", 1, 2).unwrap();
        assert_eq!(page1.columns, vec![
            Column { name: "id".into(), type_: "INTEGER".into() },
            Column { name: "name".into(), type_: "TEXT".into() },
        ]);
        assert_eq!(page1.total, Some(4));
        assert_eq!(page1.rows, vec![vec!["2".to_string(), "Bo".into()], vec!["3".into(), "Cy".into()]]);

        // Out-of-range offset → empty window, not an error.
        assert!(page(&p, "people", 100, 10).unwrap().rows.is_empty());
        // Unknown table → error.
        assert!(page(&p, "nope", 0, 10).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn spreadsheet_sources_and_paged_rows() {
        use rust_xlsxwriter::Workbook;
        let d = scratch("xlsx");
        let f = d.join("book.xlsx");
        {
            let mut wb = Workbook::new();
            let sheet = wb.add_worksheet();
            sheet.write_string(0, 0, "Name").unwrap();
            sheet.write_string(0, 1, "Age").unwrap();
            for (i, (n, a)) in [("Ann", 30.0), ("Bo", 25.0), ("Cy", 40.0)].iter().enumerate() {
                let r = (i + 1) as u32;
                sheet.write_string(r, 0, *n).unwrap();
                sheet.write_number(r, 1, *a).unwrap();
            }
            wb.save(&f).unwrap();
        }
        let p = f.to_string_lossy().to_string();
        assert_eq!(sources(&p).unwrap(), vec!["Sheet1".to_string()]);

        let pg = page(&p, "Sheet1", 1, 1).unwrap();
        assert_eq!(pg.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Name", "Age"]);
        assert_eq!(pg.total, Some(3)); // 3 data rows (header excluded)
        assert_eq!(pg.rows.len(), 1);
        assert_eq!(pg.rows[0][0], "Bo");
        // Empty source defaults to the first sheet.
        assert_eq!(page(&p, "", 0, 10).unwrap().rows.len(), 3);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn parquet_schema_and_paged_rows() {
        use parquet::data_type::Int64Type;
        use parquet::file::properties::WriterProperties;
        use parquet::file::writer::SerializedFileWriter;
        use parquet::schema::parser::parse_message_type;
        use std::sync::Arc;

        let d = scratch("parquet");
        let f = d.join("data.parquet");
        {
            let schema = Arc::new(parse_message_type("message s { REQUIRED INT64 n; }").unwrap());
            let props = Arc::new(WriterProperties::builder().build());
            let file = std::fs::File::create(&f).unwrap();
            let mut writer = SerializedFileWriter::new(file, schema, props).unwrap();
            let mut rg = writer.next_row_group().unwrap();
            let mut col = rg.next_column().unwrap().unwrap();
            col.typed::<Int64Type>().write_batch(&[10, 20, 30, 40], None, None).unwrap();
            col.close().unwrap();
            rg.close().unwrap();
            writer.close().unwrap();
        }
        let p = f.to_string_lossy().to_string();
        // Parquet is a single source → empty list.
        assert!(sources(&p).unwrap().is_empty());

        let pg = page(&p, "", 1, 2).unwrap();
        assert_eq!(pg.columns.len(), 1);
        assert_eq!(pg.columns[0].name, "n");
        assert_eq!(pg.total, Some(4));
        assert_eq!(pg.rows.len(), 2);
        // Row values render (formatting is the parquet crate's; just assert the window skipped the first).
        assert!(pg.rows[0][0].contains("20"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn unsupported_extension_errors() {
        assert!(sources("/x/notes.txt").is_err());
        assert!(page("/x/notes.txt", "", 0, 10).is_err());
    }
}
