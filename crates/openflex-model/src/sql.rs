//! A read-only SQL subset over the model's global tables.
//!
//! FlexSim lets modellers call `query()` with a SQL statement to look things up
//! in a global table instead of looping over every row by hand. OpenFlex keeps
//! a deliberately small, side-effect-free subset:
//!
//! ```text
//! SELECT <column> [, <column> …] | *
//! FROM   <table>
//! [WHERE <condition>]
//! [ORDER BY <column> [ASC | DESC]]
//! [LIMIT <n>]
//! ```
//!
//! * Only global tables (`model.global_tables`) can be queried; there are no
//!   joins, sub-queries, grouping or aggregates.
//! * `WHERE` is the normal OpenFlex expression language ([`crate::expr::Expr`]):
//!   a column name resolves to that row's cell value, `$Name` still reads a
//!   global variable and engine paths (`time`, `item.type`) resolve through the
//!   caller's [`EvalContext`].
//! * The result is **text**: the selected cells of every matching row, cells
//!   joined with `|`, rows joined with `\n`, in `ORDER BY`/insertion order. An
//!   empty table or a filter with no match yields the empty string (not an
//!   error), so `len(query("…"))` is a natural character count and
//!   `query(…) == ""` means "no rows".
//!
//! Every problem is an explicit [`SqlError`] that names the offending fragment
//! (`unknown table 'Shipments'`, `unknown column 'prio' in table 'Shipments'`,
//! `expected FROM in "SELECT *"`…). [`check`] runs the same parse and name
//! resolution for `validate`, so a bad query is reported before a run starts.

use crate::expr::{EvalContext, Expr, PickListLookup, Value};
use crate::model::{GlobalTable, TableCell};

/// A parsed `query()` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SqlQuery {
    /// Selected column names; `None` means `SELECT *` (every column).
    pub columns: Option<Vec<String>>,
    /// Table name after `FROM`.
    pub table: String,
    /// `WHERE` condition source, re-parsed as an expression.
    pub condition: Option<String>,
    /// `ORDER BY` column and direction (`true` = descending).
    pub order_by: Option<(String, bool)>,
    /// `LIMIT` row count.
    pub limit: Option<usize>,
}

/// A SQL-subset error. The [`Display`](std::fmt::Display) form is what the
/// expression layer reports and what `validate` prints, so both stay identical.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SqlError(pub String);

impl SqlError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Parse a SQL subset statement.
pub fn parse(source: &str) -> Result<SqlQuery, SqlError> {
    let text = source.trim();
    if text.is_empty() {
        return Err(SqlError::new("empty query: expected SELECT … FROM …"));
    }
    let Some(rest) = strip_keyword(text, "select") else {
        return Err(SqlError::new(format!(
            "expected SELECT at the start of \"{text}\""
        )));
    };
    let from = find_top_level_keyword(rest, "from")
        .ok_or_else(|| SqlError::new(format!("expected FROM in \"{text}\"")))?;
    let select_list = rest[..from].trim();
    if select_list.is_empty() {
        return Err(SqlError::new(format!(
            "expected a column list in \"{text}\""
        )));
    }
    let columns = if select_list == "*" {
        None
    } else {
        let mut columns = Vec::new();
        for part in select_list.split(',') {
            let name = part.trim();
            if !is_identifier(name) {
                return Err(SqlError::new(format!(
                    "invalid column name \"{name}\" in \"{text}\""
                )));
            }
            columns.push(name.to_string());
        }
        Some(columns)
    };

    let after_from = rest[from + "from".len()..].trim_start();
    let table_len = after_from
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(after_from.len());
    let table = &after_from[..table_len];
    if table.is_empty() {
        return Err(SqlError::new(format!(
            "expected a table name in \"{text}\""
        )));
    }
    let mut tail = after_from[table_len..].trim();

    let mut query = SqlQuery {
        columns,
        table: table.to_string(),
        condition: None,
        order_by: None,
        limit: None,
    };

    if let Some(rest) = strip_keyword(tail, "where") {
        let end = find_clause_start(rest, "order", Some("by"))
            .into_iter()
            .chain(find_clause_start(rest, "limit", None))
            .min()
            .unwrap_or(rest.len());
        let condition = rest[..end].trim();
        if condition.is_empty() {
            return Err(SqlError::new(format!(
                "empty WHERE condition in \"{text}\""
            )));
        }
        query.condition = Some(condition.to_string());
        tail = rest[end..].trim();
    }

    if let Some(rest) = strip_keyword(tail, "order") {
        let Some(rest) = strip_keyword(rest, "by") else {
            return Err(SqlError::new(format!(
                "expected BY after ORDER in \"{text}\""
            )));
        };
        let end = find_clause_start(rest, "limit", None).unwrap_or(rest.len());
        let spec = rest[..end].trim();
        let mut words = spec.split_whitespace();
        let column = words.next().unwrap_or_default();
        if !is_identifier(column) {
            return Err(SqlError::new(format!(
                "expected an ORDER BY column in \"{text}\""
            )));
        }
        let descending = match words.next() {
            None => false,
            Some(word) if word.eq_ignore_ascii_case("asc") => false,
            Some(word) if word.eq_ignore_ascii_case("desc") => true,
            Some(other) => {
                return Err(SqlError::new(format!(
                    "expected ASC or DESC, found \"{other}\" in \"{text}\""
                )));
            }
        };
        if words.next().is_some() {
            return Err(SqlError::new(format!(
                "unexpected input after ORDER BY in \"{text}\""
            )));
        }
        query.order_by = Some((column.to_string(), descending));
        tail = rest[end..].trim();
    }

    if let Some(rest) = strip_keyword(tail, "limit") {
        let count = rest.trim();
        match count.parse::<usize>() {
            Ok(limit) => query.limit = Some(limit),
            Err(_) => {
                return Err(SqlError::new(format!(
                    "LIMIT needs a non-negative integer, found \"{count}\" in \"{text}\""
                )));
            }
        }
        tail = "";
    }

    if !tail.is_empty() {
        return Err(SqlError::new(format!(
            "unexpected input \"{tail}\" in \"{text}\""
        )));
    }
    Ok(query)
}

/// Parse the query and check that the table (and every named column) exists.
///
/// Used by `validate` so a query over a missing table/column is reported before
/// the first event; the error text is the same as at run time.
pub fn check(globals: &dyn EvalContext, source: &str) -> Result<(), SqlError> {
    let query = parse(source)?;
    let table = table_of(globals, &query)?;
    select_columns(&query, &query.table, table)?;
    if let Some((column, _)) = &query.order_by {
        column_index(table, &query.table, column)?;
    }
    if let Some(condition) = &query.condition {
        Expr::parse(condition).map_err(|error| {
            SqlError::new(format!(
                "WHERE condition is not a valid expression: {error} (in \"{source}\")"
            ))
        })?;
    }
    Ok(())
}

/// Execute a SQL subset statement against the context's global tables.
///
/// The result is the text described in the module documentation; an empty
/// result is `""`, never an error.
pub fn execute(globals: &dyn EvalContext, source: &str) -> Result<String, SqlError> {
    let query = parse(source)?;
    let table = table_of(globals, &query)?;
    let columns = select_columns(&query, &query.table, table)?;
    let condition = match &query.condition {
        Some(source) => Some(Expr::parse(source).map_err(|error| {
            SqlError::new(format!(
                "WHERE condition is not a valid expression: {error} (in \"{source}\")"
            ))
        })?),
        None => None,
    };

    let mut rows: Vec<&Vec<TableCell>> = Vec::with_capacity(table.rows.len());
    for (row_position, row) in table.rows.iter().enumerate() {
        if let Some(condition) = &condition {
            let context = RowContext {
                table,
                row,
                outer: globals,
            };
            let matched = condition.eval(&context).map_err(|error| match error {
                // Inside WHERE a bare name is a column; if neither the row nor
                // the engine resolves it, name the column problem explicitly.
                crate::expr::ExprError::UnknownVariable(name) => SqlError::new(format!(
                    "unknown column '{name}' in table '{}'",
                    query.table
                )),
                other => SqlError::new(format!(
                    "WHERE failed on row {row_position} of '{}': {other}",
                    query.table
                )),
            })?;
            if !matched.as_bool() {
                continue;
            }
        }
        rows.push(row);
    }

    if let Some((column, descending)) = &query.order_by {
        let index = column_index(table, &query.table, column)?;
        rows.sort_by(|left, right| {
            let ordering = compare_cells(left.get(index), right.get(index));
            if *descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    }
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }

    let mut text = String::new();
    for (row_position, row) in rows.iter().enumerate() {
        if row_position > 0 {
            text.push('\n');
        }
        for (column_position, index) in columns.iter().enumerate() {
            if column_position > 0 {
                text.push('|');
            }
            text.push_str(&row.get(*index).map(TableCell::as_text).unwrap_or_default());
        }
    }
    Ok(text)
}

fn table_of<'a>(
    globals: &'a dyn EvalContext,
    query: &SqlQuery,
) -> Result<&'a GlobalTable, SqlError> {
    globals
        .global_table(&query.table)
        .ok_or_else(|| SqlError::new(format!("unknown table '{}'", query.table)))
}

fn select_columns(
    query: &SqlQuery,
    table_name: &str,
    table: &GlobalTable,
) -> Result<Vec<usize>, SqlError> {
    match &query.columns {
        None => Ok((0..table.columns.len()).collect()),
        Some(columns) => columns
            .iter()
            .map(|column| column_index(table, table_name, column))
            .collect(),
    }
}

fn column_index(table: &GlobalTable, table_name: &str, column: &str) -> Result<usize, SqlError> {
    table
        .columns
        .iter()
        .position(|name| name == column)
        .ok_or_else(|| SqlError::new(format!("unknown column '{column}' in table '{table_name}'")))
}

/// A per-row [`EvalContext`]: column names resolve to cells, everything else is
/// delegated to the caller's context (`$Name`, `time`, `item.*`).
struct RowContext<'a> {
    table: &'a GlobalTable,
    row: &'a Vec<TableCell>,
    outer: &'a dyn EvalContext,
}

impl EvalContext for RowContext<'_> {
    fn lookup(&self, path: &str) -> Option<Value> {
        if let Some(index) = self.table.columns.iter().position(|name| name == path) {
            return self.row.get(index).map(cell_value);
        }
        self.outer.lookup(path)
    }

    fn global_variable(&self, name: &str) -> Option<Value> {
        self.outer.global_variable(name)
    }

    fn picklist_entry(&self, list: &str, entry: &str) -> PickListLookup {
        self.outer.picklist_entry(list, entry)
    }

    fn global_table(&self, name: &str) -> Option<&GlobalTable> {
        self.outer.global_table(name)
    }

    fn supports_global_tables(&self) -> bool {
        self.outer.supports_global_tables()
    }
}

fn cell_value(cell: &TableCell) -> Value {
    match cell {
        TableCell::Number(value) => Value::Number(*value),
        TableCell::Bool(value) => Value::Bool(*value),
        TableCell::Str(value) => Value::Str(value.clone()),
    }
}

/// Order cells: numbers numerically, then booleans, then text; a missing cell
/// sorts first.
fn compare_cells(left: Option<&TableCell>, right: Option<&TableCell>) -> std::cmp::Ordering {
    match (left, right) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => match (left, right) {
            (TableCell::Number(a), TableCell::Number(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (TableCell::Bool(a), TableCell::Bool(b)) => a.cmp(b),
            (a, b) => rank(a)
                .cmp(&rank(b))
                .then_with(|| a.as_text().cmp(&b.as_text())),
        },
    }
}

fn rank(cell: &TableCell) -> u8 {
    match cell {
        TableCell::Number(_) => 0,
        TableCell::Bool(_) => 1,
        TableCell::Str(_) => 2,
    }
}

/// True for SQL identifiers used in this subset: letters, digits and `_`, not
/// starting with a digit.
fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Strip a leading keyword (case-insensitive) plus the following whitespace.
fn strip_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let text = text.trim_start();
    let head = text.get(..keyword.len())?;
    if !head.eq_ignore_ascii_case(keyword) {
        return None;
    }
    match text[keyword.len()..].chars().next() {
        Some(c) if c.is_alphanumeric() || c == '_' => None,
        _ => Some(text[keyword.len()..].trim_start()),
    }
}

/// Byte offsets where `keyword` starts at paren depth 0 and outside quotes.
fn find_top_level_keyword(text: &str, keyword: &str) -> Option<usize> {
    find_keyword_positions(text, keyword).next()
}

/// Offset of a clause keyword.
///
/// `require` optionally demands the next word (`ORDER` must be followed by
/// `BY`, so a `WHERE` that mentions the word "order" in a value is not split),
/// and `LIMIT` is only a clause when a number follows it.
fn find_clause_start(text: &str, keyword: &str, require: Option<&str>) -> Option<usize> {
    find_keyword_positions(text, keyword).find(|offset| {
        let after = text[offset + keyword.len()..].trim_start();
        match require {
            Some(next) => {
                let Some(head) = after.get(..next.len()) else {
                    return false;
                };
                if !head.eq_ignore_ascii_case(next) {
                    return false;
                }
                let following = after[next.len()..].chars().next();
                !matches!(following, Some(c) if c.is_alphanumeric() || c == '_')
            }
            // LIMIT only starts a clause when a row count follows.
            None => {
                if keyword.eq_ignore_ascii_case("limit") {
                    after.chars().next().is_some_and(|c| c.is_ascii_digit())
                } else {
                    true
                }
            }
        }
    })
}

/// All byte offsets of `keyword` at paren depth 0, outside quoted text, on a
/// word boundary.
fn find_keyword_positions<'a>(text: &'a str, keyword: &'a str) -> impl Iterator<Item = usize> + 'a {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut previous: Option<char> = None;
    let mut offsets = Vec::new();
    for (index, c) in text.char_indices() {
        let starts_here = match quote {
            Some(open) => {
                if c == open {
                    quote = None;
                }
                false
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    false
                }
                '(' => {
                    depth += 1;
                    false
                }
                ')' => {
                    depth -= 1;
                    false
                }
                _ => depth == 0,
            },
        };
        if starts_here {
            let word_boundary = previous.is_none_or(|p| !(p.is_alphanumeric() || p == '_'));
            if word_boundary
                && let Some(candidate) = text.get(index..index + keyword.len())
                && candidate.eq_ignore_ascii_case(keyword)
            {
                let after = text[index + keyword.len()..].chars().next();
                if !matches!(after, Some(c) if c.is_alphanumeric() || c == '_') {
                    offsets.push(index);
                }
            }
        }
        previous = Some(c);
    }
    offsets.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn table() -> GlobalTable {
        GlobalTable {
            columns: vec!["name".into(), "prio".into(), "zone".into()],
            rows: vec![
                vec![
                    TableCell::Str("box".into()),
                    TableCell::Number(2.0),
                    TableCell::Str("north".into()),
                ],
                vec![
                    TableCell::Str("tray".into()),
                    TableCell::Number(9.0),
                    TableCell::Str("south".into()),
                ],
                vec![
                    TableCell::Str("crate".into()),
                    TableCell::Number(5.0),
                    TableCell::Str("north".into()),
                ],
            ],
        }
    }

    struct Ctx(BTreeMap<String, GlobalTable>);

    impl EvalContext for Ctx {
        fn lookup(&self, _path: &str) -> Option<Value> {
            None
        }

        fn global_table(&self, name: &str) -> Option<&GlobalTable> {
            self.0.get(name)
        }

        fn supports_global_tables(&self) -> bool {
            true
        }
    }

    fn ctx() -> Ctx {
        Ctx(BTreeMap::from([("Shipments".to_string(), table())]))
    }

    #[test]
    fn parses_every_clause() {
        let query =
            parse("SELECT name, prio FROM Shipments WHERE prio > 2 ORDER BY prio DESC LIMIT 2")
                .expect("parses");
        assert_eq!(query.columns, Some(vec!["name".into(), "prio".into()]));
        assert_eq!(query.table, "Shipments");
        assert_eq!(query.condition.as_deref(), Some("prio > 2"));
        assert_eq!(query.order_by, Some(("prio".into(), true)));
        assert_eq!(query.limit, Some(2));
    }

    #[test]
    fn executes_and_formats_rows() {
        let text = execute(&ctx(), "SELECT name, prio FROM Shipments").expect("runs");
        assert_eq!(text, "box|2\ntray|9\ncrate|5");
        let text = execute(&ctx(), "SELECT * FROM Shipments LIMIT 2").expect("runs");
        assert_eq!(text, "box|2|north\ntray|9|south");
    }

    #[test]
    fn filters_and_sorts() {
        let text = execute(
            &ctx(),
            "SELECT name FROM Shipments WHERE zone == \"north\" ORDER BY prio DESC",
        )
        .expect("runs");
        assert_eq!(text, "crate\nbox");
    }

    #[test]
    fn empty_results_are_not_errors() {
        assert_eq!(
            execute(&ctx(), "SELECT * FROM Shipments WHERE prio > 99").unwrap(),
            ""
        );
        let empty = Ctx(BTreeMap::from([(
            "Empty".to_string(),
            GlobalTable::default(),
        )]));
        assert_eq!(execute(&empty, "SELECT * FROM Empty").unwrap(), "");
    }

    #[test]
    fn errors_name_the_fragment() {
        let unknown_table = execute(&ctx(), "SELECT * FROM Ghost").expect_err("unknown table");
        assert_eq!(unknown_table.to_string(), "unknown table 'Ghost'");

        let unknown_column = execute(&ctx(), "SELECT nope FROM Shipments").expect_err("column");
        assert_eq!(
            unknown_column.to_string(),
            "unknown column 'nope' in table 'Shipments'"
        );

        let condition_column =
            execute(&ctx(), "SELECT * FROM Shipments WHERE nope > 1").expect_err("where column");
        assert_eq!(
            condition_column.to_string(),
            "unknown column 'nope' in table 'Shipments'"
        );

        let syntax = parse("SELECT * Shipments").expect_err("missing FROM");
        assert!(syntax.to_string().contains("expected FROM"), "{syntax}");
        let limit = parse("SELECT * FROM Shipments LIMIT two").expect_err("bad limit");
        assert!(limit.to_string().contains("LIMIT needs"), "{limit}");
    }
}
