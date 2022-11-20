//! Run database tests

use allocation_poc::{Capabilities, SystemAllocation};

use chrono::{Duration, Utc};
use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn planned_outage(pool: PgPool) -> Result<(), anyhow::Error> {
    let planner = SystemAllocation::new(pool);

    let system = Uuid::new_v4();
    planner
        .declare_system(system, 1, Capabilities::all())
        .await?;

    let start = Utc::now();
    let end = start + Duration::hours(6);
    planner.insert_planned_outage(system, start, end).await?;

    // Adding with overlap should fail
    let result = planner
        .insert_planned_outage(system, start + Duration::hours(2), end + Duration::hours(2))
        .await;
    assert!(result.is_err());

    // Adding new outage on same timestamp as previous ended should be ok
    planner
        .insert_planned_outage(system, end, end + Duration::hours(2))
        .await?;

    // Adding new outage on same timestamp as preious started should be ok
    planner
        .insert_planned_outage(system, start - Duration::hours(2), start)
        .await?;

    // Adding one that overlaps with all three now should also fail
    let result = planner
        .insert_planned_outage(system, start - Duration::hours(1), end + Duration::hours(1))
        .await;
    assert!(result.is_err());

    Ok(())
}

#[sqlx::test]
async fn unplanned_outage(pool: PgPool) -> Result<(), anyhow::Error> {
    let planner = SystemAllocation::new(pool);

    let system = Uuid::new_v4();
    planner
        .declare_system(system, 1, Capabilities::all())
        .await?;

    let window = Duration::hours(24);
    let start = Utc::now();

    // Add fixture entry to be in the way
    planner
        .insert_entry(
            system,
            start + window,
            start + window + Duration::minutes(15),
            Capabilities::A,
        )
        .await?;

    // Attempting to insert unplanned outage in conflict within specified window
    // is not allowed.
    let result = planner
        .insert_unplanned_outage(system, start, window + Duration::minutes(5))
        .await;
    assert!(result.is_err());

    // Inserting the unplanned outage with the window prior to, or at the boundary, is ok.
    planner
        .insert_unplanned_outage(system, start, window)
        .await?;

    // inserting any entry now, regardless of prior to window or after window, is denied.
    let result = planner
        .insert_entry(
            system,
            start,
            start + Duration::minutes(15),
            Capabilities::A,
        )
        .await;
    assert!(result.is_err());

    let result = planner
        .insert_entry(
            system,
            start + window + Duration::seconds(1),
            start + window + Duration::minutes(15),
            Capabilities::A,
        )
        .await;
    assert!(result.is_err());

    Ok(())
}

#[sqlx::test]
async fn planned_capability_outage(pool: PgPool) -> Result<(), anyhow::Error> {
    let planner = SystemAllocation::new(pool);

    let system = Uuid::new_v4();
    planner
        .declare_system(system, 10, Capabilities::all())
        .await?;

    let start = Utc::now();
    let end = start + Duration::hours(6);

    // Inserting fixture entry with capability A
    planner
        .insert_entry(system, start, end, Capabilities::A)
        .await?;

    // Adding overlapping outage for capability A over existing entry should fail
    let result = planner
        .insert_planned_capability_outage(system, Capabilities::A, start, end)
        .await;
    assert!(result.is_err());

    // Adding overlapping outage for capability B should succeed
    planner
        .insert_planned_capability_outage(system, Capabilities::B, start, end + Duration::hours(24))
        .await?;

    // Adding overlapping entry with same capabilities over existing capability outage should faill
    let offset = Duration::hours(1);
    let result = planner
        .insert_entry(
            system,
            start + offset,
            end + offset,
            Capabilities::A | Capabilities::B,
        )
        .await;
    assert!(result.is_err());

    // Adding overlapping entry with different capabilities over existing capability outage
    planner
        .insert_entry(system, start + offset, end + offset, Capabilities::A)
        .await?;

    Ok(())
}

#[sqlx::test]
async fn entries_single_capacity_system(pool: PgPool) -> Result<(), anyhow::Error> {
    let planner = SystemAllocation::new(pool);

    let system = Uuid::new_v4();
    planner
        .declare_system(system, 1, Capabilities::all())
        .await?;

    let start = Utc::now();
    let end = start + Duration::minutes(15);

    planner
        .insert_entry(system, start, end, Capabilities::A)
        .await?;
    // Exact overlap is an error
    assert!(planner
        .insert_entry(system, start, end, Capabilities::A)
        .await
        .is_err());
    // Partial overlap is an error
    assert!(planner
        .insert_entry(
            system,
            start + Duration::minutes(10),
            end + Duration::minutes(10),
            Capabilities::A
        )
        .await
        .is_err());

    Ok(())
}

#[sqlx::test]
async fn entries_multiple_capacity_system(pool: PgPool) -> Result<(), anyhow::Error> {
    let planner = SystemAllocation::new(pool);

    let capacity = 6;
    let system = Uuid::new_v4();
    planner
        .declare_system(system, capacity, Capabilities::all())
        .await?;

    let start = Utc::now();
    let end = start + Duration::minutes(15);

    let mut rng = rand::thread_rng();
    for _ in 0..capacity {
        // Pick a random offset of 14 minutes
        let offset = Duration::minutes(rng.gen_range(0..15));
        planner
            .insert_entry(system, start + offset, end + offset, Capabilities::A)
            .await?;
    }

    // This entire range should now be filled
    // - it should be impossible to fit anything around the end mark
    let result = planner
        .insert_entry(
            system,
            end - Duration::seconds(30),
            end + Duration::minutes(2),
            Capabilities::A,
        )
        .await;
    assert!(result.is_err());

    Ok(())
}
