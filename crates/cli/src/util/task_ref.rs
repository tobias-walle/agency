pub fn parse_task_ref(s: &str) -> agency_core::rpc::TaskRef {
  if let Ok(id) = s.parse::<u64>() {
    agency_core::rpc::TaskRef { id: Some(id), slug: None }
  } else {
    agency_core::rpc::TaskRef { id: None, slug: Some(s.to_string()) }
  }
}
