# CLI listing integration tests

Run from the repository root:

```sh
cargo test -p arkived --test listing
```

The tests launch the compiled `arkived` binary and a loopback HTTP fixture
server. Each invocation uses a disposable working/configuration directory,
an explicit fixture endpoint, a synthetic SAS query and JSON output. The child
process does not inherit cloud credential environment variables or use a saved
account. No Azure account or Azurite installation is required.

Coverage:

- No path dispatches to container listing, including two continuation pages.
- A container/prefix adds a trailing slash and uses a directory delimiter.
- Recursive listing omits the delimiter and returns blob records.
- Blob pagination retains prefix/delimiter and forwards a marker containing
  reserved characters correctly through URL encoding.
- Empty container/blob results are machine-readable JSON arrays.
- An authorization denial exits unsuccessfully with the service error code and
  message, emits no successful JSON output and does not print the synthetic SAS.

These tests exercise CLI argument dispatch, the actual Azure REST backend,
response parsing, pagination and output serialization against controlled
responses. They do not establish real Azure authentication, service behavior,
Azurite interoperability, performance or every output format. They do not
close the original feature issue or represent maintainer acceptance.

On Windows, the child process retains only the system `SystemRoot` variable in addition to its temporary Arkived configuration. Clearing that operating-system variable breaks networking; cloud credentials and saved account settings are still excluded.
