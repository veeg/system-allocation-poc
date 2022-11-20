//! A multi-capacity, moving timeslot allocation, with partial outages.
//!
//! This mechanism ensures:
//! - A single point in time is only covered by N entries.
//! - There are multiple capabilities where some may have total outage, disallowing any entries at
//! that point in time.
//! - An unplanned outage will disallow overlaps in a sliding window forward.
//!     * On insert, all entries in conflict from start of unplanned outage plus the sliding window
//!     must be cleared prior to allowing the unplanned outage to be entered.
//!     * It is NOT possible to _add_ entries outside the window
//!     * it SHOULD be possible to _modify_ entries outside window
//!
//!  - A contineous job should run to pick up any entries that fall within the window,
//!  by forcfully removing them.
//!

use bitflags::bitflags;
use chrono::{DateTime, Duration, Utc};
use sqlx::postgres::{types::PgInterval, PgPool};
use uuid::Uuid;

bitflags! {
    #[derive(Default)]
    pub struct Capabilities: u32 {
        const A = 0b00000001;
        const B = 0b00000010;
        const C = 0b00000100;
    }
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "allocation_kind", rename_all = "lowercase")]
enum AllocationKind {
    Entry,
    Full,
    Capability,
}

pub struct SystemAllocation {
    pool: PgPool,
}

impl SystemAllocation {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl SystemAllocation {
    pub async fn declare_system(
        &self,
        system: Uuid,
        capacity: i32,
        capabilities: Capabilities,
    ) -> Result<(), anyhow::Error> {
        sqlx::query!(
            r#"
        INSERT INTO systems(system_id, capacity, capabilities) VALUES ($1, $2, $3)
            "#,
            system,
            capacity,
            // NOTE: postgres lacks unsigned types, so lets hope this conversion is actually legit
            capabilities.bits() as i32,
        )
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from)
    }

    /// Insert a single entry to occupy a timeslot on the system.
    pub async fn insert_entry(
        &self,
        system: Uuid,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        capabilities: Capabilities,
    ) -> Result<(), anyhow::Error> {
        let allocation_id = Uuid::new_v4();
        sqlx::query!(
            r#"
        INSERT INTO entries(allocation_id, start_time, end_time)
        VALUES ($1, $2, $3)
            "#,
            allocation_id,
            start,
            end
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
        INSERT INTO allocations(system_id, allocation_id, kind, planned, start_time, end_time, capabilities)
        VALUES ($1, $2, $3, true, $4, $5, $6)
            "#,
            system, allocation_id, AllocationKind::Entry as _, start, end, capabilities.bits() as i32
        ).execute(&self.pool).await?;

        Ok(())
    }

    /// Inserting a planned outage for the duration (start, end).
    ///
    /// This outage _must_ resolve all conflicts. No partial capability downtimes allowed.
    /// This function will fail if any items are in conflict.
    pub async fn insert_planned_outage(
        &self,
        system: Uuid,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), anyhow::Error> {
        let allocation_id = Uuid::new_v4();
        let capabilities = Capabilities::all().bits() as i32;
        sqlx::query!(
            r#"
        INSERT INTO planned(allocation_id, system_id, start_time, end_time, capabilities)
        VALUES ($1, $2, $3, $4, $5)
            "#,
            allocation_id,
            system,
            start,
            end,
            capabilities,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
        INSERT INTO allocations(system_id, allocation_id, kind, planned, start_time, end_time, capabilities)
        VALUES ($1, $2, $3, true, $4, $5, $6)
            "#,
            system,
            allocation_id,
            AllocationKind::Full as _,
            start,
            end,
            capabilities,
        ).execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Only those in conflict from start + sliding_window duration will be evaluated to be
    /// enforced to re-allocate before this unplanned outage can be successfully inserted.
    pub async fn insert_unplanned_outage(
        &self,
        system: Uuid,
        start: DateTime<Utc>,
        sliding_window: Duration,
    ) -> Result<(), anyhow::Error> {
        let allocation_id = Uuid::new_v4();
        let capabilities = Capabilities::all().bits() as i32;
        sqlx::query!(
            r#"
        INSERT INTO unplanned (allocation_id, system_id, start_time, sliding_window, capabilities)
        VALUES ($1, $2, $3, $4, $5)
            "#,
            allocation_id,
            system,
            start,
            PgInterval::try_from(sliding_window).unwrap(),
            capabilities,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
        INSERT INTO allocations(system_id, allocation_id, kind, planned, start_time, end_time, capabilities)
        VALUES ($1, $2, $3, false, $4, 'infinity', $5)
            "#,
            system,
            allocation_id,
            AllocationKind::Full as _,
            start,
            capabilities,
        ).execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Insert an outage only in a single capability. All entires overlapping with the same
    /// capability must be cleared prior to inserting this.
    ///
    /// Return all entires in conflict on error.
    pub async fn insert_planned_capability_outage(
        &self,
        system: Uuid,
        capabilities: Capabilities,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), anyhow::Error> {
        let allocation_id = Uuid::new_v4();
        let capabilities = capabilities.bits() as i32;
        sqlx::query!(
            r#"
        INSERT INTO planned (allocation_id, system_id, start_time, end_time, capabilities)
        VALUES ($1, $2, $3, $4, $5)
            "#,
            allocation_id,
            system,
            start,
            end,
            capabilities,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
        INSERT INTO allocations(system_id, allocation_id, kind, planned, start_time, end_time, capabilities)
        VALUES ($1, $2, $3, true, $4, $5, $6)
            "#,
            system,
            allocation_id,
            AllocationKind::Capability as _,
            start,
            end,
            capabilities,
        ).execute(&self.pool)
            .await?;

        Ok(())
    }
}
