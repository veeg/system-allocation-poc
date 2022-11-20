# A multi-capacity, moving timeslot allocation, with partial outages.

This is a proof-of-concept database implementation with these features:

- A system may express a set of capabilities it supports.
- An entry may occupy a timespan on a system, with a set of required capabilities.
- A system may be configured with a maximum concurrent capacity of entries at any point in time.
- A _planned_ outage may be registered for the entire system, or a subset of capabilities,
with a known start and expected end time.
- All entries in conflict of the registered capabilities must be cleared prior to accepting
the _planned_ outage.
- An _unplanned_ outage may be registered with an _unknown_ end time, with a configurable
sliding window of time where conflicts must be cleared.
- All entries in conflict within the sliding window must be cleared of an _unplanned_ outage.
- All entries _outside_ the sliding window is allowed to stay put.
- Adding additional entries to a system when an outage is present is disallowed, regardless
of type.
  * With an outage, we do not want to allow entries to occupy time on the system.
  * For unplanned events, the expected resolve time may vary, and we may not want to commit
  to allowing additional load in the future if the issue has not been resolved by then.
- Modifying an entry outside the sliding window is allowed.
  * For an unplanned outage, we are uncertain of the given resolve time, and should allow
  flexibility by not over-eagerly removing or denying modifications to far into the future,
  when one can only reach a final conclusion after some time after the initial unplanned outage
  was registered. One may require to modify the unplanned outage.

- A continuous job should run to pick up any entries that fall within the sliding window
of an unplanned outage, by forcefully removing them from the allocation table.


## TODO:
- implement continuous job to remove entries within outage window.
- implement entry modification operations
- implement outage modification operations
- implement proper error propagation to correctly identify the error conditions, and resources in conflict.

## Running tests

This POC uses [sqlx](https://docs.rs/sqlx), which requires `DATABASE_URL` to be set. This is
automated by the `.env` file checked into this repository, which points to the container
administrated by using `docker-compose up`.

To compile anything, either run using `SQLX_OFFLINE=true` (using the `sqlx-data,json` query cache),
or run the migrations yourself against the local database with `sqlx database reset -y`
