# Historical state fixtures

`first-release.json` contains harmless first-party data in actual historical formats:

- Registry, lock and seven-field installation receipt: source definitions at commit `1573284` (`model.rs`, `install.rs`).
- Job record without `steps` or `retry_of`: source writer at commit `c4bf2ba` (`jobs.rs`).

Values are synthetic and contain no user data, credentials or live endpoint. No historical binary is executed. Tests substitute `__OWNED_DESTINATION__` with their isolated project-local directory. The known owned payload is exactly `First-release owned skill.\r\n`, SHA-256 `66399534741ec58daccae674af4bb47941a6db9bb0a268e440de26fccb672af3`.

The shared-service tests cover reading v1 state, retaining registry/lock data and old receipt identity through current lifecycle, preserving user edits, retrying a job written before progress tracking, and refusing unsupported versions without rewriting the file. Existing lifecycle tests already cover legacy inline-journal recovery. These fixtures prove compatibility with the specified historical shapes, not an invented v0 migration or arbitrary future schema.
