# unreleased 0.2.0
## Remove
- remove `prepare`, `query`, `execute`, `query_raw`, `execute_raw`, `query_simple` and `execute_simple` methods from all types. Leave only `Execute` trait as sole query API  
    ```rust
    use xitca_postgres::{Client, Execute, RowSimpleStream, RowStream, Statement};
    // create a named statement and execute it. on success returns a prepared statement
    let stmt: StatementGuarded<'_, Client> = Statement::named("SELECT 1").execute(&client).await?;
    // query with the prepared statement. on success returns an async row stream.
    let stream: RowStream<'_> = stmt.query(&client)?;
    // query with raw string sql.
    let stream: RowSimpleStream<'_> = "SELECT 1; SELECT 1".query(&client)?;
    // execute raw string sql.
    let row_affected: u64 = "SELECT 1; SELECT 1".execute(&client).await?;
    ```
- remove `dev::AsParams` trait export. It's not needed for implementing `Query` trait anymore    

## Change
- query with parameter value arguments must be bind to it's `Statement` before calling `Execute` methods.
    ```rust
    use xitca_postgres::Execute;
    // prepare a statement.
    let stmt = Statement::named("SELECT * FROM users WHERE id = $1 AND age = $2", &[Type::INT4, Type::INT4]).execute(&client).await?;
    // bind statement to typed value and start query
    let stream = stmt.bind([9527, 42]).query(&client)?;
    ```
- query without parameter value can be queried with `Statement` alone.
    ```rust
    use xitca_postgres::Execute;
    // prepare a statement.
    let stmt = Statement::named("SELECT * FROM users", &[]).execute(&client).await?;
    // statement have no value params and can be used for query.
    let stream = stmt.query(&client)?;
    ```
- `AsyncLendingIterator` is no longer exported from crate's root path. use `iter::AsyncLendingIterator` instead
- `query::RowStreamOwned` and `row::RowOwned` are no longer behind `compat` crate feature anymore
- `statement::Statement::unnamed` must bind to value parameters with `bind` or `bind_dyn` before calling `Execute` methods.
    ```rust
    let stmt = Statement::unnamed("SELECT * FROM users WHERE id = $1", &[Type::INT4]);
    let row_stream = stmt.bind([9527]).query(&client);
    ```
- `Query::_send_encode_query` method's return type is changed to `Result<(<S as Encode>::Output<'_>, Response), Error>`. Enabling further simplify of the surface level API at the cost of more internal complexity
- `Encode` trait implementation detail change.
- `IntoStream` trait is renamed to `IntoResponse`

## Add
- add `Execute` trait for extending query customization
- add `Client::prepare_blocking`
- add `Prepare::{_prepare_blocking, _get_type_blocking}`
- add `iter::AsyncLendingIteratorExt` for extending async iterator APIs
- add `statement::Statement::{bind, bind_dyn}` methods for binding value parameters to a prepared statement for query
- add `query::RowSimpleStreamOwned`
- add `error::DriverIoErrorMulti` type for outputting read and write IO errors at the same time

## Fix
- remove `Clone` trait impl from `Statement`. this is a bug where `Statement` type is not meant to be duplicateable by library user
