//! Structured-data previews (CPE-088/089/090/091): read-only text summaries of a SQLite database, a
//! spreadsheet workbook (XLSX/ODS), and a Parquet file for the preview pane. Reads only metadata /
//! bounded content so it stays cheap. rusqlite bundles SQLite (no system lib); calamine + parquet are
//! pure-Rust. Extracted into the Server (CPE-815); the Tauri `read_preview_info` command dispatches here.

use std::fs;

/// Read-only summary of a SQLite database: its tables/views, each with a row count and column list.
pub fn sqlite_info(path: &str) -> Result<String, String> {
    use rusqlite::{Connection, OpenFlags};
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

    let items: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT name, type FROM sqlite_master \
                 WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut out = format!("SQLite database — {} table(s)/view(s)\n\n", items.len());
    for (name, kind) in &items {
        // Names come from sqlite_master; double-quote and escape for safety.
        let quoted = format!("\"{}\"", name.replace('"', "\"\""));
        let columns: Vec<String> = {
            let mut cols = Vec::new();
            if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({quoted})")) {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
                    cols = rows.filter_map(|r| r.ok()).collect();
                }
            }
            cols
        };
        let detail = if kind == "table" {
            match conn.query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |r| r.get::<_, i64>(0)) {
                Ok(c) => format!("{c} rows"),
                Err(_) => "unreadable".to_string(),
            }
        } else {
            "view".to_string()
        };
        out.push_str(&format!("{name} ({kind}) — {detail}\n"));
        if !columns.is_empty() {
            out.push_str(&format!("  columns: {}\n", columns.join(", ")));
        }
    }
    Ok(out)
}

/// Read-only text-grid preview of a spreadsheet workbook (XLSX/ODS) via calamine: each sheet rendered as
/// tab-separated rows, capped.
pub fn spreadsheet_info(path: &str) -> Result<String, String> {
    use calamine::{open_workbook_auto, Reader};
    const MAX_ROWS: usize = 100;
    const MAX_COLS: usize = 20;

    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let names: Vec<String> = wb.sheet_names().iter().map(|s| s.to_string()).collect();
    let mut out = format!("Workbook — {} sheet(s): {}\n", names.len(), names.join(", "));

    for name in &names {
        if let Ok(range) = wb.worksheet_range(name) {
            let (h, w) = (range.height(), range.width());
            out.push_str(&format!("\n=== {name} ({h} x {w}) ===\n"));
            for row in range.rows().take(MAX_ROWS) {
                let cells: Vec<String> = row.iter().take(MAX_COLS).map(|c| c.to_string()).collect();
                out.push_str(&cells.join("\t"));
                out.push('\n');
            }
            if h > MAX_ROWS {
                out.push_str(&format!("… {} more rows\n", h - MAX_ROWS));
            }
        }
    }
    Ok(out)
}

/// Read-only schema summary of a Parquet file via the parquet crate's footer metadata — no full column
/// scan.
pub fn parquet_info(path: &str) -> Result<String, String> {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = SerializedFileReader::new(file).map_err(|e| e.to_string())?;
    let meta = reader.metadata();
    let fmeta = meta.file_metadata();
    let schema = fmeta.schema_descr();
    let mut out = format!(
        "Parquet — {} rows, {} row group(s)\nCreated by: {}\n\nColumns ({}):\n",
        fmeta.num_rows(),
        meta.num_row_groups(),
        fmeta.created_by().unwrap_or("(unknown)"),
        schema.num_columns()
    );
    for i in 0..schema.num_columns() {
        let col = schema.column(i);
        out.push_str(&format!("  {} : {:?}\n", col.name(), col.physical_type()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-datprev-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sqlite_info_lists_tables_rows_and_columns() {
        use rusqlite::Connection;
        let d = scratch("sqlite");
        let f = d.join("test.db");
        {
            let conn = Connection::open(&f).unwrap();
            conn.execute_batch(
                "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT);
                 INSERT INTO people (name) VALUES ('Ann'), ('Bo');",
            )
            .unwrap();
        }
        let info = sqlite_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("people (table) — 2 rows"), "table + row count: {info:?}");
        assert!(info.contains("columns: id, name"), "column list present");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn spreadsheet_info_renders_cells_as_a_grid() {
        use rust_xlsxwriter::Workbook;
        let d = scratch("xlsx");
        let f = d.join("book.xlsx");
        {
            let mut wb = Workbook::new();
            let sheet = wb.add_worksheet();
            sheet.write_string(0, 0, "Name").unwrap();
            sheet.write_string(0, 1, "Age").unwrap();
            sheet.write_string(1, 0, "Ann").unwrap();
            sheet.write_number(1, 1, 30.0).unwrap();
            wb.save(&f).unwrap();
        }
        let info = spreadsheet_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("Workbook — 1 sheet"), "sheet count: {info:?}");
        assert!(info.contains("Name\tAge"), "header row rendered tab-separated");
        assert!(info.contains("Ann\t30"), "data row rendered");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn parquet_info_errors_on_a_non_parquet() {
        let d = scratch("parquet_bad");
        let f = d.join("x.parquet");
        fs::write(&f, b"not a parquet file").unwrap();
        assert!(parquet_info(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
