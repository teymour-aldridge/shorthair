-- Your SQL goes here
create unique index spar_series_join_request_email_unique on spar_series_join_requests (spar_series_id, email);

create unique index spar_series_join_request_name_unique on spar_series_join_requests (spar_series_id, name);
