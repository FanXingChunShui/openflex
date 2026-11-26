//! A read-only SQL subset over the model's global tables.
//!
//! FlexSim lets modellers call `query()` with a SQL statement to look things up
//! in a global table instead of looping over every row by hand. OpenFlex keeps
//! a deliberately small, side-effect-free subset:
//!
//! ```text
//! SELECT  COUNT(*) | SUM(<col>) | AVG(<col>) | MIN(<col>) | MAX(<col>)
//!         | <col> [, …] | *
//! FROM    <table>
//! [WHERE  <condition>]
//! [GROUP BY <col> [, …]]
//! [ORDER BY <col> | <aggregate> [ASC | DESC]]
//! [LIMIT  <n>]
//! ```
//!
//! * Only global tables (`model.global_tables`) can be queried; **joins and
//!   sub-queries are not supported**. Aggregates and `GROUP BY` are.
//! * `WHERE` is the normal OpenFlex expression language ([`crate::expr::Expr`]):
//!   a column name resolves to that row's cell value, `$Name` still reads a
//!   global variable and engine paths (`time`, `item.type`) resolve through the
//!   caller's [`EvalContext`].
//! * The result is **text**: the selected cells of every output row, cells
//!   joined with `|`, rows joined with `\n`. Aggregates use the same shortest
//!   number formatting as table cells. Without `GROUP BY`, a query containing
//!   aggregates produces exactly one output row; **grouped** output rows follow
//!   the insertion order of each group's first matching row (unless `ORDER BY`).
//! * Aggregate and ordinary columns may be mixed only when every ordinary
//!   selected column is in `GROUP BY`; otherwise the error says
//!   `aggregate used without GROUP BY` / `column 'x' must appear in GROUP BY`.
//! * Empty input: `COUNT(*)` is `0`, `SUM` is `0`, `AVG`/`MIN`/`MAX` are the
//!   empty string. A `GROUP BY` query with no matching rows yields the empty
//!   string (0 rows).
//! * `SUM`/`AVG`/`MIN`/`MAX` require numeric cells (booleans count as 0/1);
//!   a string cell is an error naming the 1-based table row and the column. It
//!   is never skipped or counted as zero.
//! * `ORDER BY` accepts a column or an aggregate written in function form
//!   (`count(*)`, `sum(col)`, …); the aggregate need not appear in `SELECT`.
//!   Positional `ORDER BY 1` is not supported.
//!
//! Every problem is an explicit [`SqlError`] that names the offending fragment
//! (`unknown table 'Shipments'`, `unknown column 'prio' in table 'Shipments'`,
//! `expected FROM in "SELECT *"`…). [`check`] runs the same parse, name
//! resolution and aggregate rules for `validate`, so a bad query is reported
//! before a run starts. `check` scans the whole table for a numeric aggregate's
//! column, which is deliberately conservative with respect to `WHERE`.

use crate::expr::{EvalContext, Expr, PickListLookup, Value};
use crate::model::{GlobalTable, TableCell};

/// Aggregate functions supported by the SQL subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AggregateFunction {
    fn name(self) -> &'static str {
        match self {
            AggregateFunction::Count => "COUNT",
            AggregateFunction::Sum => "SUM",
            AggregateFunction::Avg => "AVG",
            AggregateFunction::Min => "MIN",
            AggregateFunction::Max => "MAX",
        }
    }

    fn numeric(self) -> bool {
        !matches!(self, AggregateFunction::Count)
    }
}

/// One item in the `SELECT` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectItem {
    /// A plain column.
    Column(String),
    /// An aggregate; `column` is `None` only for `COUNT(*)`.
    Aggregate {
        function: AggregateFunction,
        column: Option<String>,
    },
}

/// A parsed `query()` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SqlQuery {
    /// Simple selected column names. `None` means `SELECT *`; an aggregate-only
    /// query has `Some(vec![])`. Kept for compatibility with older callers;
    /// [`SqlQuery::select`] is the authoritative item list.
    pub columns: Option<Vec<String>>,
    /// Selected items in output order.
    pub select: Vec<SelectItem>,
    /// Table name after `FROM`.
    pub table: String,
    /// `WHERE` condition source, re-parsed as an expression.
    pub condition: Option<String>,
    /// `GROUP BY` columns.
    pub group_by: Vec<String>,
    /// Plain `ORDER BY` column and direction (`true` = descending).
    pub order_by: Option<(String, bool)>,
    /// Aggregate `ORDER BY` expression and direction (`true` = descending).
    pub order_by_aggregate: Option<(AggregateFunction, Option<String>, bool)>,
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
    let mut select = Vec::new();
    let columns = if select_list == "*" {
        None
    } else {
        for part in split_top_level(select_list, ',') {
            let name = part.trim();
            select.push(parse_select_item(name, text)?);
        }
        Some(
            select
                .iter()
                .filter_map(|item| match item {
                    SelectItem::Column(name) => Some(name.clone()),
                    SelectItem::Aggregate { .. } => None,
                })
                .collect(),
        )
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
        select,
        table: table.to_string(),
        condition: None,
        group_by: Vec::new(),
        order_by: None,
        order_by_aggregate: None,
        limit: None,
    };

    if let Some(rest) = strip_keyword(tail, "where") {
        let end = find_clause_start(rest, "group", Some("by"))
            .into_iter()
            .chain(find_clause_start(rest, "order", Some("by")))
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

    if let Some(rest) = strip_keyword(tail, "group") {
        let Some(rest) = strip_keyword(rest, "by") else {
            return Err(SqlError::new(format!(
                "expected BY after GROUP in \"{text}\""
            )));
        };
        let end = find_clause_start(rest, "order", Some("by"))
            .into_iter()
            .chain(find_clause_start(rest, "limit", None))
            .min()
            .unwrap_or(rest.len());
        let spec = rest[..end].trim();
        if spec.is_empty() {
            return Err(SqlError::new(format!(
                "expected a GROUP BY column in \"{text}\""
            )));
        }
        for part in split_top_level(spec, ',') {
            let name = part.trim();
            if !is_identifier(name) {
                return Err(SqlError::new(format!(
                    "invalid GROUP BY column \"{name}\" in \"{text}\""
                )));
            }
            query.group_by.push(name.to_string());
        }
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
        let target = words.next().unwrap_or_default();
        if target.is_empty() {
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
        if target.contains('(') {
            let (function, column) = parse_aggregate(target, text)?;
            query.order_by_aggregate = Some((function, column, descending));
        } else if is_identifier(target) {
            query.order_by = Some((target.to_string(), descending));
        } else {
            return Err(SqlError::new(format!(
                "expected an ORDER BY column in \"{text}\""
            )));
        }
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

fn parse_select_item(text: &str, source: &str) -> Result<SelectItem, SqlError> {
    if is_identifier(text) {
        return Ok(SelectItem::Column(text.to_string()));
    }
    if text.contains('(') {
        let (function, column) = parse_aggregate(text, source)?;
        return Ok(SelectItem::Aggregate { function, column });
    }
    Err(SqlError::new(format!(
        "invalid column name \"{text}\" in \"{source}\""
    )))
}

/// Parse `COUNT(*)` or `SUM(col)` / `AVG` / `MIN` / `MAX`.
fn parse_aggregate(
    text: &str,
    source: &str,
) -> Result<(AggregateFunction, Option<String>), SqlError> {
    let Some(open) = text.find('(') else {
        return Err(SqlError::new(format!(
            "unknown aggregate function \"{text}\" in \"{source}\""
        )));
    };
    let name = text[..open].trim();
    let rest = text[open + 1..].trim();
    if !rest.ends_with(')') {
        return Err(SqlError::new(format!(
            "invalid aggregate \"{text}\" in \"{source}\""
        )));
    }
    let argument = rest[..rest.len() - 1].trim();
    let function = match name.to_ascii_uppercase().as_str() {
        "COUNT" => AggregateFunction::Count,
        "SUM" => AggregateFunction::Sum,
        "AVG" => AggregateFunction::Avg,
        "MIN" => AggregateFunction::Min,
        "MAX" => AggregateFunction::Max,
        other => {
            return Err(SqlError::new(format!(
                "unknown aggregate function '{other}' in \"{source}\""
            )));
        }
    };
    if function == AggregateFunction::Count {
        if argument != "*" {
            return Err(SqlError::new(format!(
                "COUNT only supports COUNT(*) in \"{source}\""
            )));
        }
        return Ok((function, None));
    }
    if !is_identifier(argument) {
        return Err(SqlError::new(format!(
            "invalid aggregate column \"{argument}\" in \"{source}\""
        )));
    }
    Ok((function, Some(argument.to_string())))
}

/// Parse the query and check that the table, columns, aggregates and
/// `GROUP BY` rules are valid.
///
/// Used by `validate` so a query over a missing table/column, an unknown
/// aggregate or an aggregate without its `GROUP BY` is reported before the
/// first event; the error text is the same as at run time.
pub fn check(globals: &dyn EvalContext, source: &str) -> Result<(), SqlError> {
    let query = parse(source)?;
    let table = table_of(globals, &query)?;
    let plan = build_plan(&query, &query.table, table)?;
    validate_order(&query, &plan, &query.table, table)?;
    if let Some(condition) = &query.condition {
        Expr::parse(condition).map_err(|error| {
            SqlError::new(format!(
                "WHERE condition is not a valid expression: {error} (in \"{source}\")"
            ))
        })?;
    }
    check_numeric_cells(&plan, table, &query.table)?;
    Ok(())
}

/// Execute a SQL subset statement against the context's global tables.
///
/// The result is the text described in the module documentation; an empty
/// result is `""`, never an error.
pub fn execute(globals: &dyn EvalContext, source: &str) -> Result<String, SqlError> {
    let query = parse(source)?;
    let table = table_of(globals, &query)?;
    let plan = build_plan(&query, &query.table, table)?;
    validate_order(&query, &plan, &query.table, table)?;
    let condition = match &query.condition {
        Some(source) => Some(Expr::parse(source).map_err(|error| {
            SqlError::new(format!(
                "WHERE condition is not a valid expression: {error} (in \"{source}\")"
            ))
        })?),
        None => None,
    };

    let mut rows: Vec<(usize, &Vec<TableCell>)> = Vec::with_capacity(table.rows.len());
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
        rows.push((row_position, row));
    }

    let grouped = !plan.group_by.is_empty();
    let has_aggregate = plan.items.iter().any(|item| match item {
        ResolvedItem::Aggregate(..) => true,
        ResolvedItem::Column(_) => false,
    });
    let mut groups: Vec<RowGroup<'_>> = if grouped {
        let mut groups: Vec<KeyedGroup<'_>> = Vec::new();
        for (row_position, row) in rows {
            let key: Vec<TableCell> = plan
                .group_by
                .iter()
                .map(|&index| {
                    row.get(index)
                        .cloned()
                        .unwrap_or(TableCell::Str(String::new()))
                })
                .collect();
            if let Some((_, members)) = groups.iter_mut().find(|(existing, _)| *existing == key) {
                members.push((row_position, row));
            } else {
                groups.push((key, vec![(row_position, row)]));
            }
        }
        groups.into_iter().map(|(_, members)| members).collect()
    } else if has_aggregate {
        vec![rows]
    } else {
        rows.into_iter().map(|row| vec![row]).collect()
    };

    if let Some((column, descending)) = &query.order_by {
        if grouped {
            let index = column_index(table, &query.table, column)?;
            if !plan.group_by.contains(&index) {
                return Err(SqlError::new(format!(
                    "ORDER BY column '{column}' must appear in GROUP BY"
                )));
            }
        }
        let index = column_index(table, &query.table, column)?;
        groups.sort_by(|left, right| {
            let left_cell = left.first().and_then(|(_, row)| row.get(index));
            let right_cell = right.first().and_then(|(_, row)| row.get(index));
            let ordering = compare_cells(left_cell, right_cell);
            if *descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
    } else if let Some((function, column, descending)) = &query.order_by_aggregate {
        let column_index = match column {
            Some(column) => Some(column_index(table, &query.table, column)?),
            None => None,
        };
        let mut keyed: Vec<ScoredGroup<'_>> = Vec::with_capacity(groups.len());
        for group in groups {
            let value = aggregate(*function, column_index, &group, table, &query.table)?;
            let number = value.as_number().unwrap_or(0.0);
            keyed.push((number, group));
        }
        keyed.sort_by(|left, right| {
            let ordering = left
                .0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal);
            if *descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        groups = keyed.into_iter().map(|(_, group)| group).collect();
    }

    if let Some(limit) = query.limit {
        groups.truncate(limit);
    }

    let mut text = String::new();
    for (group_position, group) in groups.iter().enumerate() {
        if group_position > 0 {
            text.push('\n');
        }
        for (column_position, item) in plan.items.iter().enumerate() {
            if column_position > 0 {
                text.push('|');
            }
            match item {
                ResolvedItem::Column(index) => {
                    text.push_str(
                        group
                            .first()
                            .and_then(|(_, row)| row.get(*index))
                            .map(TableCell::as_text)
                            .unwrap_or_default()
                            .as_str(),
                    );
                }
                ResolvedItem::Aggregate(function, column) => {
                    let value = aggregate(*function, *column, group, table, &query.table)?;
                    text.push_str(&value.as_text());
                }
            }
        }
    }
    Ok(text)
}

#[derive(Debug, Clone, Copy)]
enum ResolvedItem {
    Column(usize),
    Aggregate(AggregateFunction, Option<usize>),
}

struct QueryPlan {
    items: Vec<ResolvedItem>,
    group_by: Vec<usize>,
}

type RowRef<'a> = (usize, &'a Vec<TableCell>);
type RowGroup<'a> = Vec<RowRef<'a>>;
type KeyedGroup<'a> = (Vec<TableCell>, RowGroup<'a>);
type ScoredGroup<'a> = (f64, RowGroup<'a>);

fn build_plan(
    query: &SqlQuery,
    table_name: &str,
    table: &GlobalTable,
) -> Result<QueryPlan, SqlError> {
    let mut items = Vec::new();
    if query.columns.is_none() && query.select.is_empty() {
        items.extend((0..table.columns.len()).map(ResolvedItem::Column));
    } else {
        for item in &query.select {
            match item {
                SelectItem::Column(name) => {
                    items.push(ResolvedItem::Column(column_index(table, table_name, name)?));
                }
                SelectItem::Aggregate { function, column } => {
                    let index = match column {
                        Some(column) => Some(column_index(table, table_name, column)?),
                        None => None,
                    };
                    items.push(ResolvedItem::Aggregate(*function, index));
                }
            }
        }
    }

    let group_by = query
        .group_by
        .iter()
        .map(|column| column_index(table, table_name, column))
        .collect::<Result<Vec<_>, _>>()?;

    let selected_columns: Vec<usize> = items
        .iter()
        .filter_map(|item| match item {
            ResolvedItem::Column(index) => Some(*index),
            ResolvedItem::Aggregate(..) => None,
        })
        .collect();
    let has_aggregate = items
        .iter()
        .any(|item| matches!(item, ResolvedItem::Aggregate(..)));

    if has_aggregate && group_by.is_empty() && !selected_columns.is_empty() {
        return Err(SqlError::new(
            "aggregate used without GROUP BY (selected ordinary columns need GROUP BY)",
        ));
    }
    if !group_by.is_empty() {
        for index in selected_columns {
            if !group_by.contains(&index) {
                return Err(SqlError::new(format!(
                    "column '{}' must appear in GROUP BY",
                    table.columns.get(index).cloned().unwrap_or_default()
                )));
            }
        }
    }

    Ok(QueryPlan { items, group_by })
}

fn validate_order(
    query: &SqlQuery,
    plan: &QueryPlan,
    table_name: &str,
    table: &GlobalTable,
) -> Result<Option<(AggregateFunction, Option<usize>, bool)>, SqlError> {
    if let Some((column, descending)) = &query.order_by {
        let index = column_index(table, table_name, column)?;
        if (!plan.group_by.is_empty()
            || plan
                .items
                .iter()
                .any(|item| matches!(item, ResolvedItem::Aggregate(..))))
            && !plan.group_by.contains(&index)
        {
            return Err(SqlError::new(format!(
                "ORDER BY column '{column}' must appear in GROUP BY"
            )));
        }
        let _ = (index, descending);
        return Ok(None);
    }
    if let Some((function, column, descending)) = &query.order_by_aggregate {
        let index = match column {
            Some(column) => Some(column_index(table, table_name, column)?),
            None => None,
        };
        return Ok(Some((*function, index, *descending)));
    }
    Ok(None)
}

fn check_numeric_cells(
    plan: &QueryPlan,
    table: &GlobalTable,
    table_name: &str,
) -> Result<(), SqlError> {
    for item in &plan.items {
        if let ResolvedItem::Aggregate(function, Some(index)) = item
            && function.numeric()
            && let Some(column) = table.columns.get(*index)
        {
            for (row_position, row) in table.rows.iter().enumerate() {
                if let Some(cell) = row.get(*index)
                    && cell.as_number().is_none()
                {
                    return Err(SqlError::new(format!(
                        "non-numeric cell \"{}\" in column '{column}' at row {} of '{table_name}'",
                        cell.as_text(),
                        row_position + 1
                    )));
                }
            }
        }
    }
    Ok(())
}

fn aggregate(
    function: AggregateFunction,
    column: Option<usize>,
    group: &[(usize, &Vec<TableCell>)],
    table: &GlobalTable,
    table_name: &str,
) -> Result<TableCell, SqlError> {
    if function == AggregateFunction::Count {
        return Ok(TableCell::Number(group.len() as f64));
    }
    let Some(index) = column else {
        return Err(SqlError::new(format!(
            "{} needs a column in '{table_name}'",
            function.name()
        )));
    };
    if group.is_empty() {
        return Ok(match function {
            AggregateFunction::Sum => TableCell::Number(0.0),
            _ => TableCell::Str(String::new()),
        });
    }
    let column_name = table.columns.get(index).cloned().unwrap_or_default();
    let mut sum = 0.0;
    let mut count = 0usize;
    let mut best: Option<f64> = None;
    for (row_position, row) in group {
        let Some(cell) = row.get(index) else {
            continue;
        };
        let Some(value) = cell.as_number() else {
            return Err(SqlError::new(format!(
                "non-numeric cell \"{}\" in column '{column_name}' at row {} of '{table_name}'",
                cell.as_text(),
                row_position + 1
            )));
        };
        sum += value;
        count += 1;
        best = Some(match (function, best) {
            (AggregateFunction::Min, Some(current)) => current.min(value),
            (AggregateFunction::Max, Some(current)) => current.max(value),
            _ => value,
        });
    }
    if count == 0 {
        return Ok(match function {
            AggregateFunction::Sum => TableCell::Number(0.0),
            _ => TableCell::Str(String::new()),
        });
    }
    Ok(match function {
        AggregateFunction::Sum => TableCell::Number(sum),
        AggregateFunction::Avg => TableCell::Number(sum / count as f64),
        AggregateFunction::Min | AggregateFunction::Max => TableCell::Number(best.unwrap_or(0.0)),
        AggregateFunction::Count => TableCell::Number(count as f64),
    })
}

fn table_of<'a>(
    globals: &'a dyn EvalContext,
    query: &SqlQuery,
) -> Result<&'a GlobalTable, SqlError> {
    globals
        .global_table(&query.table)
        .ok_or_else(|| SqlError::new(format!("unknown table '{}'", query.table)))
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

/// Split on `separator` at paren depth 0 and outside quotes.
fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (index, c) in text.char_indices() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if c == separator && depth == 0 => {
                parts.push(&text[start..index]);
                start = index + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
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

    /// Malformed queries must come back as errors, never as panics.
    ///
    /// `query()` is called from model expressions, so a panic would take the
    /// whole run down instead of reporting a bad query. The parser is hand
    /// written, which makes this a cheap guard against index arithmetic and
    /// slicing bugs.
    #[test]
    fn malformed_queries_never_panic() {
        let cases = [
            "",
            " ",
            "SELECT",
            "SELECT *",
            "SELECT * FROM",
            "SELECT * FROM Missing",
            "SELECT * FROM Shipments WHERE",
            "SELECT * FROM Shipments WHERE 1 1",
            "SELECT * FROM Shipments ORDER BY",
            "SELECT * FROM Shipments ORDER BY missing",
            "SELECT * FROM Shipments LIMIT",
            "SELECT * FROM Shipments LIMIT -1",
            "SELECT * FROM Shipments LIMIT abc",
            "SELECT ( FROM Shipments",
            "SELECT COUNT( FROM Shipments",
            "SELECT COUNT(*) FROM",
            "SELECT COUNT(*) FROM Shipments GROUP BY",
            "SELECT COUNT(*), name FROM Shipments",
            "SELECT SUM(name) FROM Shipments",
            "SELECT MEDIAN(prio) FROM Shipments",
            "SELECT * FROM Shipments GROUP BY name, name",
            "SELECT * FROM Shipments; DROP TABLE Shipments",
            "\u{0}",
            "SELECT * FROM Shipments WHERE prio > 'unterminated",
        ];
        for case in cases {
            // Neither path may panic; both must return *something*.
            let _ = parse(case);
            match check(&ctx(), case) {
                Ok(()) => {}
                Err(error) => {
                    assert!(
                        !error.to_string().trim().is_empty(),
                        "an error must say something: {case:?}"
                    );
                }
            }
            if let Err(error) = execute(&ctx(), case) {
                assert!(!error.to_string().trim().is_empty(), "{case:?}");
            }
        }
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

    fn empty_table() -> GlobalTable {
        GlobalTable {
            columns: vec!["name".into(), "prio".into(), "zone".into()],
            rows: Vec::new(),
        }
    }

    fn mixed_table() -> GlobalTable {
        GlobalTable {
            columns: vec!["name".into(), "prio".into()],
            rows: vec![
                vec![TableCell::Str("box".into()), TableCell::Number(2.0)],
                vec![TableCell::Str("tray".into()), TableCell::Str("n/a".into())],
            ],
        }
    }

    #[test]
    fn count_star_respects_where() {
        assert_eq!(
            execute(
                &ctx(),
                "SELECT COUNT(*) FROM Shipments WHERE zone == \"north\""
            )
            .expect("count"),
            "2"
        );
    }

    #[test]
    fn sum_avg_min_max_format_numbers() {
        assert_eq!(
            execute(&ctx(), "SELECT SUM(prio) FROM Shipments").expect("sum"),
            "16"
        );
        let average = execute(&ctx(), "SELECT AVG(prio) FROM Shipments").expect("avg");
        assert!((average.parse::<f64>().expect("number") - 16.0 / 3.0).abs() < 1e-12);
        assert_eq!(
            execute(&ctx(), "SELECT MIN(prio) FROM Shipments").expect("min"),
            "2"
        );
        assert_eq!(
            execute(&ctx(), "SELECT MAX(prio) FROM Shipments").expect("max"),
            "9"
        );
    }

    #[test]
    fn group_by_aggregates_in_first_sighting_order() {
        assert_eq!(
            execute(
                &ctx(),
                "SELECT zone, COUNT(*), SUM(prio) FROM Shipments GROUP BY zone"
            )
            .expect("groups"),
            "north|2|7\nsouth|1|9"
        );
    }

    #[test]
    fn empty_input_aggregates_use_the_documented_defaults() {
        let empty = Ctx(BTreeMap::from([("Empty".to_string(), empty_table())]));
        assert_eq!(
            execute(&empty, "SELECT COUNT(*) FROM Empty").expect("count"),
            "0"
        );
        assert_eq!(
            execute(&empty, "SELECT SUM(prio) FROM Empty").expect("sum"),
            "0"
        );
        for function in ["AVG", "MIN", "MAX"] {
            let sql = format!("SELECT {function}(prio) FROM Empty");
            assert_eq!(execute(&empty, &sql).expect("empty"), "");
        }
        assert_eq!(
            execute(&empty, "SELECT zone, COUNT(*) FROM Empty GROUP BY zone").expect("groups"),
            ""
        );
        assert_eq!(
            execute(&ctx(), "SELECT SUM(prio) FROM Shipments WHERE prio > 99").expect("sum"),
            "0"
        );
    }

    #[test]
    fn aggregate_without_group_by_and_plain_column_is_rejected() {
        let error =
            execute(&ctx(), "SELECT zone, COUNT(*) FROM Shipments").expect_err("must reject");
        assert!(
            error
                .to_string()
                .contains("aggregate used without GROUP BY"),
            "{error}"
        );
        let error = execute(&ctx(), "SELECT name, COUNT(*) FROM Shipments GROUP BY zone")
            .expect_err("ungrouped column");
        assert!(
            error.to_string().contains("must appear in GROUP BY"),
            "{error}"
        );
    }

    #[test]
    fn sum_on_a_string_cell_names_row_and_column() {
        let mixed = Ctx(BTreeMap::from([("Mixed".to_string(), mixed_table())]));
        let error = execute(&mixed, "SELECT SUM(prio) FROM Mixed").expect_err("string sum");
        let message = error.to_string();
        assert!(message.contains("column 'prio'"), "{message}");
        assert!(message.contains("row 2"), "{message}");
    }

    #[test]
    fn unknown_aggregate_function_is_rejected() {
        let error = parse("SELECT MEDIAN(prio) FROM Shipments").expect_err("unknown aggregate");
        assert!(
            error
                .to_string()
                .contains("unknown aggregate function 'MEDIAN'"),
            "{error}"
        );
    }

    #[test]
    fn order_by_aggregate_sorts_groups() {
        assert_eq!(
            execute(
                &ctx(),
                "SELECT zone, COUNT(*) FROM Shipments GROUP BY zone ORDER BY count(*) ASC"
            )
            .expect("ordered groups"),
            "south|1\nnorth|2"
        );
    }

    #[test]
    fn check_rejects_bad_aggregates() {
        let in_ctx = Ctx(BTreeMap::from([
            ("Shipments".to_string(), table()),
            ("Mixed".to_string(), mixed_table()),
        ]));
        for sql in [
            "SELECT zone, COUNT(*) FROM Shipments",
            "SELECT MEDIAN(prio) FROM Shipments",
            "SELECT SUM(nope) FROM Shipments",
            "SELECT SUM(prio) FROM Mixed",
        ] {
            assert!(check(&in_ctx, sql).is_err(), "check should reject {sql}");
        }
    }
}
