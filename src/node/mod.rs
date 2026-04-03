// Shim: all node types live in the `zihuan_node` workspace crate.
// Re-export the entire public surface so existing `crate::node::*` paths keep working.
pub use zihuan_node::*;

// Re-expose sub-modules so paths like `crate::node::data_value::X`,
// `crate::node::graph_io::Y`, `crate::node::util::Z` etc. remain valid.
pub mod brain_tool_spec {
    pub use zihuan_node::brain_tool_spec::*;
}
pub mod data_value {
    pub use zihuan_node::data_value::*;
}
pub mod database {
    pub use zihuan_node::database::*;
    pub mod mysql {
        pub use zihuan_node::database::mysql::*;
    }
    pub mod redis {
        pub use zihuan_node::database::redis::*;
    }
}
pub mod function_graph {
    pub use zihuan_node::function_graph::*;
}
pub mod graph_io {
    pub use zihuan_node::graph_io::*;
}
pub mod message_cache {
    pub use zihuan_node::message_cache::*;
}
pub mod message_mysql_get_group_history {
    pub use zihuan_node::message_mysql_get_group_history::*;
}
pub mod message_mysql_get_user_history {
    pub use zihuan_node::message_mysql_get_user_history::*;
}
pub mod message_mysql_history_common {
    pub use zihuan_node::message_mysql_history_common::*;
}
pub mod message_mysql_persistence {
    pub use zihuan_node::message_mysql_persistence::*;
}
pub mod qq_message_list_mysql_persistence {
    pub use zihuan_node::qq_message_list_mysql_persistence::*;
}
pub mod registry {
    pub use zihuan_node::registry::*;
    // init_node_registry lives in crate::init_registry, not here.
}
pub mod util {
    pub use zihuan_node::util::*;
    pub mod format_string {
        pub use zihuan_node::util::format_string::*;
    }
    pub mod function {
        pub use zihuan_node::util::function::*;
    }
    pub mod set_variable {
        pub use zihuan_node::util::set_variable::*;
    }
    pub mod string_data {
        pub use zihuan_node::util::string_data::*;
    }
    pub mod session_state_try_claim {
        pub use zihuan_node::util::session_state_try_claim::*;
    }
}
