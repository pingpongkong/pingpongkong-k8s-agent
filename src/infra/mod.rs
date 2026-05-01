mod kubernetes;

pub use kubernetes::{discover_role_ips, get_my_roles, get_node_metadata};
