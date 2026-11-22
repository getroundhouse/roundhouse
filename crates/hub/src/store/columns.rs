//! What a query reads, declared once for each kind of row, so the list a statement selects and the name each reader
//! takes a value by come from the same declaration. Removing or renaming a column there fails to compile where it's
//! read, and an error from reading one names it. Every name is a place in the row, fixed as the crate compiles, so
//! nothing looks a column up by its name as the hub runs. A statement that writes the row gives each value by its
//! column's name too, so the order of the declaration's columns changes nothing that's stored. `clippy.toml` refuses
//! rusqlite's reads of a column by its number. [`ByColumn`] makes the only ones of a query the hub writes itself, so a
//! query of several values has a declaration, and a query of one value is read with [`OneValue::read`]. A reply whose
//! columns SQLite fixes, such as a `PRAGMA`'s, or a test's read of a table in its own order, is read by number where
//! it's made, with an `#[expect]` that says which it is.

use rusqlite::Row;
use rusqlite::types::{FromSql, Type, ValueRef};
use std::error::Error;
use std::fmt;

/// A column of what a query reads: its place in the row, which its declaration gives, and its name there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Column {
    index: usize,
    name: &'static str,
}

impl Column {
    /// Only [`columns!`] makes one, so a column's place is always the one its declaration gives it.
    pub(crate) const fn declared(index: usize, name: &'static str) -> Self {
        Self { index, name }
    }

    /// Its name, as its declaration spells it.
    pub(super) fn name(self) -> &'static str {
        self.name
    }
}

/// What a declaration of columns gives, besides a [`Column`] for each, as [`columns!`] makes it.
pub(crate) trait Columns {
    /// Every column, as a statement selects them, in their order.
    const LIST: &'static str;
    /// The row's own columns, before those a read adds after them, such as an event's raw line, for a statement that
    /// reads the row and then adds its own.
    const ROW: &'static str;
    /// How many columns [`Columns::LIST`] has, which is where a list read after it starts.
    const COUNT: usize;
}

/// Where a reader finds the value it reads: a declared [`Column`], or [`OneValue`].
pub(crate) trait At: Copy {
    fn index(self) -> usize;
    /// The error for what this column holds that its reader can't read.
    fn unreadable(self, kind: Type, error: Box<dyn Error + Send + Sync>) -> rusqlite::Error;
}

impl At for Column {
    fn index(self) -> usize {
        self.index
    }

    fn unreadable(self, kind: Type, error: Box<dyn Error + Send + Sync>) -> rusqlite::Error {
        rusqlite::Error::FromSqlConversionFailure(self.index, kind, Box::new(Unreadable { column: self.name, error }))
    }
}

/// The one value a query reads, such as `SELECT rev FROM accounts WHERE id = ?`, whose read sits beside it with no
/// order to keep, so it has no declaration. A reader takes no bare number, so a query of several values declares them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OneValue;

impl OneValue {
    /// Reads the one value, passed to a query in place of a closure: `conn.query_row(sql, params, OneValue::read)`.
    pub(crate) fn read<T: FromSql>(row: &Row<'_>) -> rusqlite::Result<T> {
        row.at(Self)
    }
}

impl At for OneValue {
    fn index(self) -> usize {
        0
    }

    fn unreadable(self, kind: Type, error: Box<dyn Error + Send + Sync>) -> rusqlite::Error {
        rusqlite::Error::FromSqlConversionFailure(0, kind, error)
    }
}

/// Reads a row's values by their declared [`Column`], or its [`OneValue`].
pub(crate) trait ByColumn {
    fn at<T: FromSql>(&self, column: impl At) -> rusqlite::Result<T>;
    /// What `column` holds, borrowed from the row rather than copied.
    fn borrowed_at(&self, column: Column) -> rusqlite::Result<ValueRef<'_>>;
}

#[expect(
    clippy::disallowed_methods,
    reason = "this is where a reader reads a declared column or a query's one value, at the place its declaration gives"
)]
impl ByColumn for Row<'_> {
    fn at<T: FromSql>(&self, column: impl At) -> rusqlite::Result<T> {
        self.get(column.index())
    }

    fn borrowed_at(&self, column: Column) -> rusqlite::Result<ValueRef<'_>> {
        self.get_ref(column.index)
    }
}

/// What a column held that its reader couldn't read, with the column's declared name.
#[derive(Debug)]
pub(super) struct Unreadable {
    pub(super) column: &'static str,
    error: Box<dyn Error + Send + Sync>,
}

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "can't read the column {}: {}", self.column, self.error)
    }
}

impl Error for Unreadable {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.error)
    }
}

/// Declares the columns of what a query reads, in their order, as a struct with a [`Column`] for each, and
/// [`Columns`] for its list. A column is its name, or its name and the SQL that selects it, for one a query qualifies
/// or computes, such as `id = "p.id"`. A second group, after `then`, is what a read adds after the row's own columns,
/// such as a subquery or `rev`. `at(first)` gives the columns where the list starts at `first` in a query's, for a list
/// read after another, as an account's login is.
///
/// `written as` names a second struct, for a statement that writes the row's own columns, which are the first group,
/// and those after `with`, which the row has and this declaration doesn't read, such as a session's
/// `client_request_id`. It has a field for each, so a write gives every value by its column's name, and binds them in
/// the declaration's order.
macro_rules! columns {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($column:ident $(= $sql:literal)?),+ $(,)?
        }
        $(then {
            $($added:ident $(= $added_sql:literal)?),+ $(,)?
        })?
        $(written as $values:ident $(with { $($with:ident),+ $(,)? })?)?
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug)]
        $vis struct $name {
            $($vis $column: $crate::store::columns::Column,)+
            $($($vis $added: $crate::store::columns::Column,)+)?
        }

        impl $crate::store::columns::Columns for $name {
            const LIST: &'static str = $crate::store::columns::columns!(
                @join $($crate::store::columns::columns!(@sql $column $(= $sql)?)),+
                $($(, $crate::store::columns::columns!(@sql $added $(= $added_sql)?))+)?
            );
            const ROW: &'static str = $crate::store::columns::columns!(
                @join $($crate::store::columns::columns!(@sql $column $(= $sql)?)),+
            );
            const COUNT: usize = [$(stringify!($column),)+ $($(stringify!($added),)+)?].len();
        }

        impl $name {
            /// The columns where the list starts at `first` in the query's.
            $vis const fn at(first: usize) -> Self {
                use $crate::store::columns::Column;
                #[expect(non_camel_case_types, reason = "each is a column's name, as the declaration spells it")]
                enum Index {
                    $($column,)+
                    $($($added,)+)?
                }
                Self {
                    $($column: Column::declared(first + Index::$column as usize, stringify!($column)),)+
                    $($($added: Column::declared(first + Index::$added as usize, stringify!($added)),)+)?
                }
            }
        }

        $crate::store::columns::columns!(@values $vis $name [$($column),+] [$($values)?] [$($($($with),+)?)?]);
    };
    (@values $vis:vis $name:ident [$($column:ident),+] [] []) => {};
    (@values $vis:vis $name:ident [$($column:ident),+] [$values:ident] [$($with:ident),*]) => {
        #[doc = concat!("The value a write gives each column of [`", stringify!($name), "`]'s row.")]
        $vis struct $values<'a> {
            $($vis $column: &'a dyn rusqlite::ToSql,)+
            $($vis $with: &'a dyn rusqlite::ToSql,)*
        }

        impl<'a> $values<'a> {
            /// What a statement that inserts the row says after the table's name: the columns, then a `?` for each.
            $vis const INSERT: &'static str = concat!(
                "(",
                $crate::store::columns::columns!(@join $(stringify!($column)),+ $(, stringify!($with))*),
                ") VALUES (",
                $crate::store::columns::columns!(
                    @join $($crate::store::columns::columns!(@place $column)),+
                    $(, $crate::store::columns::columns!(@place $with))*
                ),
                ")",
            );

            /// Each value, in its column's place.
            $vis fn params(self) -> impl rusqlite::Params {
                rusqlite::params_from_iter([$(self.$column,)+ $(self.$with,)*])
            }
        }
    };
    (@join $first:expr $(, $rest:expr)*) => {
        concat!($first $(, ", ", $rest)*)
    };
    (@sql $column:ident) => {
        stringify!($column)
    };
    (@sql $column:ident = $sql:literal) => {
        $sql
    };
    (@place $column:ident) => {
        "?"
    };
}
pub(crate) use columns;

#[cfg(test)]
mod tests {
    use super::super::database::text_at;
    use super::{ByColumn, Columns, OneValue, Unreadable};
    use rusqlite::Connection;
    use rusqlite::types::Type;
    use serde::Deserialize;

    /// A closed set with no variant for what it doesn't know, so a status it doesn't list can't be read.
    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "snake_case")]
    enum Status {
        Idle,
    }

    columns! {
        struct Sample {
            id,
            number,
            status,
        } then {
            doubled = "number * 2",
            label = "upper(status)",
        }
        written as SampleValues with { note }
    }

    /// Its rows as [`SampleValues`] writes them, into a table whose columns are in another order.
    fn sample() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE samples (status TEXT, note TEXT, id TEXT, number INTEGER)").unwrap();
        let insert = format!("INSERT INTO samples {}", SampleValues::INSERT);
        for (id, number, status) in [("a", 21, "idle"), ("b", 1, "no such status")] {
            let values = SampleValues { id: &id, number: &number, status: &status, note: &format!("{id}'s") };
            conn.execute(&insert, values.params()).unwrap();
        }
        conn
    }

    #[test]
    fn a_declaration_gives_its_list_and_a_place_for_each_name() {
        assert_eq!(Sample::LIST, "id, number, status, number * 2, upper(status)");
        assert_eq!(Sample::ROW, "id, number, status");
        assert_eq!(Sample::COUNT, 5);
        assert_eq!(SampleValues::INSERT, "(id, number, status, note) VALUES (?, ?, ?, ?)");

        let conn = sample();
        let c = Sample::at(0);
        let read = conn
            .query_row(&format!("SELECT {} FROM samples WHERE id = 'a'", Sample::LIST), [], |row| {
                Ok((
                    row.at::<String>(c.id)?,
                    row.at::<i64>(c.number)?,
                    row.at::<i64>(c.doubled)?,
                    text_at::<Status>(row, c.status)?,
                    row.at::<String>(c.label)?,
                ))
            })
            .unwrap();
        assert_eq!(read, ("a".to_owned(), 21, 42, Status::Idle, "IDLE".to_owned()));

        // Each value a write gave by name is in its column, in the table's own order.
        #[expect(clippy::disallowed_methods, reason = "it reads the table in its own order on purpose")]
        let stored: (String, String, String, i64) = conn
            .query_row("SELECT * FROM samples WHERE id = 'a'", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap();
        assert_eq!(stored, ("idle".to_owned(), "a's".to_owned(), "a".to_owned(), 21));

        // The same list after one of a query's own, as a list read after another is.
        let c = Sample::at(1);
        let shifted = conn
            .query_row(&format!("SELECT 'first', {} FROM samples WHERE id = 'a'", Sample::LIST), [], |row| {
                Ok((row.at::<String>(c.id)?, row.at::<String>(c.label)?))
            })
            .unwrap();
        assert_eq!(shifted, ("a".to_owned(), "IDLE".to_owned()));

        // A query of one value, which has no declaration.
        let number: i64 = conn.query_row("SELECT number FROM samples WHERE id = 'a'", [], OneValue::read).unwrap();
        assert_eq!(number, 21);
    }

    #[test]
    fn what_a_column_cant_read_is_an_error_that_names_it() {
        let c = Sample::at(0);

        let error = sample()
            .query_row(&format!("SELECT {} FROM samples WHERE id = 'b'", Sample::LIST), [], |row| {
                text_at::<Status>(row, c.status)
            })
            .unwrap_err();

        let rusqlite::Error::FromSqlConversionFailure(index, Type::Text, error) = error else {
            panic!("expected a conversion failure, got {error:?}");
        };
        assert_eq!(index, 2);
        assert_eq!(error.downcast_ref::<Unreadable>().unwrap().column, "status");
    }
}
