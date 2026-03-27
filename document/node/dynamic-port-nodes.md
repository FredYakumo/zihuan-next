# Dynamic Port Nodes

A dynamic-port node is one whose input or output port list is determined by its inline configuration rather than being fixed at compile time.

---

## When to use dynamic ports

| Scenario | Direction |
|----------|-----------|
| Template string with `${variable}` placeholders | Dynamic **inputs** (one per variable) |
| JSON field extraction with user-configured fields | Dynamic **outputs** (one per field) |
| LLM brain node with user-configured tools | Dynamic **outputs** (one per tool) |

The pattern: the user configures something in the node's inline editor → that config is stored in an `inline_values` key → the node parses it and generates ports at runtime.

---

## Implementation steps

### Step 1: Store parsed config in the struct

```rust
pub struct MyDynamicNode {
    id: String,
    name: String,
    // The parsed configuration drives port generation
    field_defs: Vec<FieldDef>,
}

struct FieldDef {
    name: String,
    data_type: DataType,
}
```

### Step 2: Override `has_dynamic_*_ports`

```rust
impl Node for MyDynamicNode {
    // Override the direction that is dynamic
    fn has_dynamic_output_ports(&self) -> bool { true }

    // For dynamic inputs instead:
    // fn has_dynamic_input_ports(&self) -> bool { true }
```

### Step 3: Generate ports from config in `input_ports()` / `output_ports()`

```rust
    fn output_ports(&self) -> Vec<Port> {
        self.field_defs
            .iter()
            .map(|f| Port::new(f.name.clone(), f.data_type.clone()))
            .collect()
    }

    // Static ports (if any) go in input_ports:
    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("json", DataType::Json),
            Port::new("fields_config", DataType::Json).optional(),
        ]
    }
```

### Step 4: Parse config in `apply_inline_config`

```rust
    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        if let Some(DataValue::Json(config)) = inline_values.get("fields_config") {
            self.field_defs = parse_field_defs(config)?;
        }
        Ok(())
    }
```

`apply_inline_config` is called once before execution starts. After it runs, `output_ports()` must return the correct dynamic port list.

---

## Registry probe compatibility

The registry calls `output_ports()` on a freshly-constructed node (with no config applied, the "probe" instance) to read the static port metadata. For dynamic-port nodes, this probe returns an empty list for the dynamic direction — that is expected and correct.

The `dynamic_output_ports: true` / `dynamic_input_ports: true` flags in the graph JSON tell the loader and validator to skip that direction when doing compatibility checks and auto-fix.

---

## Graph JSON markers

Every dynamic-port node must have the appropriate flag in its JSON definition:

```jsonc
{
  "node_type": "json_extract",
  "dynamic_input_ports": false,
  "dynamic_output_ports": true,   // ← tells loader: output ports are config-driven
  "input_ports": [ ... ],
  "output_ports": [ ... ]         // ← actual current ports (restored from inline config)
}
```

The loader calls `refresh_port_types()` which reads the `dynamic_*_ports` flags from the live registry and writes them into the definition. Old JSON files that omit these flags are patched up automatically.

**What the flags change at load/validate time:**

| Flag | Effect |
|------|--------|
| `dynamic_input_ports: true` | Skip adding/removing input ports during auto-fix; skip input-port type mismatch warnings |
| `dynamic_output_ports: true` | Skip adding/removing output ports during auto-fix; skip output-port type mismatch warnings |

---

## UI editor coordination

When you have a dynamic-port node, the UI must keep its inline config and port list in sync. There are two places where ports get rebuilt:

1. **Runtime:** `apply_inline_config()` called by `build_node_graph_from_definition()`
2. **Editor save callback:** called when the user closes the inline editor dialog

Both must produce exactly the same port list given the same config. The typical pattern in the save callback (e.g. `json_extract_editor.rs`):

```rust
// 1. Serialize the new config to JSON and store it
node_def.inline_values.insert(
    "fields_config".to_string(),
    serde_json::to_value(&new_fields).unwrap(),
);

// 2. Rebuild output ports to match what apply_inline_config would produce
node_def.output_ports = new_fields.iter()
    .map(|f| Port { name: f.name.clone(), data_type: f.data_type.clone(), required: true, description: None })
    .collect();

// 3. Ensure the dynamic flag is set
node_def.dynamic_output_ports = true;
```

After this, call `refresh_active_tab_ui()` to re-render.

---

## Real examples in the codebase

| Node | Dynamic direction | Config key | Source |
|------|------------------|------------|--------|
| `FormatStringNode` | inputs | `template` (String) | `src/node/util/format_string.rs` |
| `JsonExtractNode` | outputs | `fields_config` (Json) | `src/node/util/json_extract.rs` |
| `BrainNode` | outputs | `tools_config` (Json) | `src/llm/brain_node.rs` |

---

## Testing dynamic port nodes

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ports_rebuild_on_config_change() {
        let mut node = MyDynamicNode::new("t".into(), "T".into());

        // Before config: no dynamic ports
        assert_eq!(node.output_ports().len(), 0);

        // After config: ports match definitions
        let mut inline = HashMap::new();
        inline.insert(
            "fields_config".to_string(),
            DataValue::Json(serde_json::json!([
                {"name": "title", "data_type": "String"},
                {"name": "count", "data_type": "Integer"},
            ])),
        );
        node.apply_inline_config(&inline).unwrap();

        let ports = node.output_ports();
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].name, "title");
        assert_eq!(ports[0].data_type, DataType::String);
        assert_eq!(ports[1].name, "count");
        assert_eq!(ports[1].data_type, DataType::Integer);
    }
}
```
