/**
 * Minimal CSV parser for the preview pane. Handles quoted fields, escaped
 * quotes (`""`), and commas/newlines inside quotes. Accepts both `\n` and
 * `\r\n` line endings. Not a full RFC-4180 validator — just enough to render a
 * readable table preview.
 */
export function parseCsv(text: string, delimiter = ","): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let inQuotes = false;

  for (let i = 0; i < text.length; i++) {
    const c = text[i];

    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i++; // consume the escaped quote
        } else {
          inQuotes = false;
        }
      } else {
        field += c;
      }
      continue;
    }

    if (c === '"') {
      inQuotes = true;
    } else if (c === delimiter) {
      row.push(field);
      field = "";
    } else if (c === "\n") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else if (c === "\r") {
      // Swallow CR; the following LF (if any) ends the row.
    } else {
      field += c;
    }
  }

  // Flush the final field/row unless the input ended exactly on a newline.
  if (field !== "" || row.length > 0) {
    row.push(field);
    rows.push(row);
  }

  return rows;
}
