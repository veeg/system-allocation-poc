-- Define the initial system table
create table if not exists systems (
    system_id uuid primary key not null,
    capacity int not null,
    capabilities int not null
);

-- Hold primary entries here
create table if not exists entries (
    allocation_id uuid primary key not null,
    start_time timestamptz not null,
    end_time timestamptz not null
);

-- Hold unplanned outages here
create table if not exists unplanned (
    allocation_id uuid primary key not null,
    system_id uuid references systems(system_id) not null,
    start_time timestamptz not null,
    sliding_window interval minute not null,
    capabilities int not null,
    resolved_at timestamptz
);

-- Hold unplanned outages here
create table if not exists planned (
    allocation_id uuid primary key not null,
    system_id uuid references systems(system_id) not null,
    start_time timestamptz not null,
    end_time timestamptz not null,
    capabilities int not null
);

create type allocation_kind as enum ('entry', 'full', 'capability');

-- Enforce everything in this table
create table allocations (
    system_id uuid references systems(system_id) not null,
    allocation_id uuid not null,
    kind allocation_kind not null,
    planned boolean default true not null,
    start_time timestamptz not null,
    end_time timestamptz not null,
    capabilities int not null
);


-- Check unplanned outages against conflicting entries in the sliding window period
create function unplanned_outage_entry_overlap_check()
    returns trigger
    language plpgsql
    as
$$
declare
    _entry_overlap_count int;
begin
    -- Our only responsiblity here is to ensure that there are no allocations
    -- that overlap with the initial insertion window.
    select count(*) from allocations
    where (new.start_time + new.sliding_window) > start_time
        and new.capabilities & capabilities != 0
        and kind = 'entry'
    into _entry_overlap_count;

    if _entry_overlap_count != 0 then
        raise exception 'cannot insert unplanned outage in conflict with entries within sliding window';
    end if;

    return new;
end;
$$;

create trigger unplanned_outage_entry_overlap_check
before insert on unplanned
for each row
execute function unplanned_outage_entry_overlap_check();

-- Check planned outages against conflicting entries for the finite duration of the outage.
create function planned_outage_entry_overlap_check()
    returns trigger
    language plpgsql
    as
$$
declare
    _entry_overlap_count int;
begin
    -- Our only responsibility is to assert that no entries with the same capabilities are
    -- in conflict for the entire finite outage timespan.
    select count(*) from allocations
    where new.start_time < end_time
        and new.end_time > start_time
        and new.capabilities & capabilities != 0
        and kind = 'entry'
    into _entry_overlap_count;

    if _entry_overlap_count != 0 then
        raise exception 'cannot insert planned outage in conflict with entries';
    end if;

    return new;
end;
$$;

create trigger planned_outage_entry_overlap_check
before insert on planned
for each row
execute function planned_outage_entry_overlap_check();

-- Post-insert check on allocations table to ensure non-overlap
create function allocation_overlap_check()
    returns trigger
    language plpgsql
    as
$$
declare
    _outage_overlaps int;
    _entry_overlaps int;
    _system_capacity int;
begin
    -- Check that the new allocation does not conflict with any existing for any _outages_
    -- This is applicable for all allocation types, even outages themselves.
    -- This is to ensure that no duplicate outage entries are added that cover the same timespan.
    select count(*)
    from allocations
    where new.start_time < end_time
        and new.end_time > start_time
        -- If any of the capabilities of the existing rows overlap this the new one
        and new.capabilities & capabilities != 0
        and kind != 'entry'
    into _outage_overlaps;

    if _outage_overlaps != 0 then
        raise exception 'cannot insert overlapping outage';
    end if;

    -- Check new 'entry' allocation for concurrent capacity violations
    if new.kind = 'entry' then
        select count(*)
        from allocations
        where new.start_time < end_time
            and new.end_time > start_time
            and kind = 'entry'
        into _entry_overlaps;

        select capacity from systems where system_id = new.system_id
        into _system_capacity;

        if (_entry_overlaps + 1) > _system_capacity then
            raise exception 'system capacity at max';
        end if;
    end if;

    return new;
end;
$$;

create trigger allocation_overlap_check
before insert on allocations
for each row
execute function allocation_overlap_check();

