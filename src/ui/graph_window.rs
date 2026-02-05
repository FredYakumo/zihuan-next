use slint::slint;

slint! {
    import { HorizontalBox, VerticalBox, ScrollView } from "std-widgets.slint";

    export struct NodeVm {
        label: string,
        x: float,
        y: float,
    }

    export struct EdgeVm {
        label: string,
    }

    export struct NodeTypeVm {
        type_id: string,
        display_name: string,
        category: string,
        description: string,
    }

    component CjkText inherits Text {
        font-family: "Heiti SC";
    }

    component CjkButton inherits Rectangle {
        in property <string> text;
        callback clicked();

        width: 120px;
        height: 32px;
        background: #2f2f2f;
        border-radius: 6px;
        border-width: 1px;
        border-color: #4a4a4a;

        TouchArea {
            clicked => { root.clicked(); }
        }

        CjkText {
            text: root.text;
            color: #f0f0f0;
            vertical-alignment: center;
            horizontal-alignment: center;
            font-size: 12px;
        }
    }

    component NodeItem inherits Rectangle {
        in property <string> label;
        in property <float> x_pos;
        in property <float> y_pos;

        x: x_pos * 1px;
        y: y_pos * 1px;
        width: 160px;
        height: 56px;
        background: #2b2b2b;
        border-radius: 8px;
        border-width: 1px;
        border-color: #4a4a4a;

        CjkText {
            text: label;
            color: #f0f0f0;
            vertical-alignment: center;
            horizontal-alignment: center;
            font-size: 14px;
        }
    }

    component GraphCanvas inherits Rectangle {
        in property <[NodeVm]> nodes;
        background: #1e1e1e;

        for node in nodes: NodeItem {
            label: node.label;
            x_pos: node.x;
            y_pos: node.y;
        }
    }

    export component NodeGraphWindow inherits Window {
        in property <[NodeVm]> nodes;
        in property <[EdgeVm]> edges;
        in property <string> current_file;
        in property <bool> show_node_selector: false;
        in property <[NodeTypeVm]> available_node_types;

        callback open_json();
        callback add_node(string /* type_id */);
        callback show_node_type_menu();
        callback hide_node_type_menu();

        title: "Zihuan Node Graph Viewer";
        width: 1200px;
        height: 800px;

        HorizontalBox {
            spacing: 12px;
            padding: 12px;

            GraphCanvas {
                width: 860px;
                height: 760px;
                nodes: root.nodes;
            }

            VerticalBox {
                width: 300px;
                height: 760px;
                spacing: 8px;

                CjkButton {
                    text: "读取节点图文件";
                    clicked => { root.open_json(); }
                }

                CjkButton {
                    text: "新增节点";
                    clicked => { root.show_node_type_menu(); }
                }

                CjkText {
                    text: root.current_file;
                    font-size: 12px;
                    color: #555555;
                    wrap: word-wrap;
                }

                CjkText {
                    text: "Edges";
                    font-size: 16px;
                    color: #222222;
                }

                for edge in edges: CjkText {
                    text: edge.label;
                    font-size: 12px;
                    color: #444444;
                }
            }
        }

        // Node type selector overlay
        if root.show_node_selector: Rectangle {
            width: 100%;
            height: 100%;
            background: #00000080;

            TouchArea {
                // Click outside to close could be implemented here if we put another rect behind the dialog
                clicked => { root.hide_node_type_menu(); }
            }

            // Dialog Frame
            Rectangle {
                x: (parent.width - self.width) / 2;
                y: (parent.height - self.height) / 2;
                width: 600px;
                height: 500px;
                background: #2b2b2b;
                border-radius: 12px;
                border-width: 2px;
                border-color: #4a4a4a;

                // Stop propagation of clicks inside the dialog
                TouchArea {}

                VerticalBox {
                    padding: 16px;
                    spacing: 12px;

                    CjkText {
                        text: "选择节点类型";
                        font-size: 18px;
                        color: #f0f0f0;
                        horizontal-alignment: center;
                        height: 30px;
                    }

                    Rectangle {
                        background: #1e1e1e;
                        border-radius: 6px;
                        border-color: #333333;
                        border-width: 1px;
                        
                        ScrollView {
                            viewport-width: parent.width;
                            viewport-height: root.available_node_types.length * 85px; 

                            VerticalBox {
                                padding: 8px;
                                spacing: 8px;
                                alignment: start;

                                for node_type in root.available_node_types: Rectangle {
                                    height: 80px;
                                    background: #2f2f2f;
                                    border-radius: 6px;
                                    border-width: 1px;
                                    border-color: #4a4a4a;

                                    TouchArea {
                                        clicked => { 
                                            root.add_node(node_type.type_id);
                                            root.hide_node_type_menu();
                                        }
                                    }

                                    VerticalLayout {
                                        padding: 10px;
                                        spacing: 6px;

                                        // Header: Name + Category
                                        HorizontalLayout {
                                            spacing: 8px;
                                            CjkText {
                                                text: node_type.display_name;
                                                font-size: 16px;
                                                color: #ffffff;
                                                font-weight: 700;
                                                vertical-alignment: center;
                                            }

                                            Rectangle {
                                                background: #3a3a3a;
                                                border-radius: 4px;
                                                // Calculate width roughly based on text length if needed, or let layout handle
                                                width: self.height * 2.5; 
                                                
                                                HorizontalLayout {
                                                    padding-left: 6px;
                                                    padding-right: 6px;
                                                    CjkText {
                                                        text: node_type.category;
                                                        font-size: 11px;
                                                        color: #aaaaaa;
                                                        vertical-alignment: center;
                                                        horizontal-alignment: center;
                                                    }
                                                }
                                            }
                                        }

                                        // Description
                                        CjkText {
                                            text: node_type.description;
                                            font-size: 13px;
                                            color: #bbbbbb;
                                            wrap: word-wrap;
                                            vertical-alignment: top;
                                            overflow: elide;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    HorizontalBox {
                        alignment: center;
                        CjkButton {
                            text: "取消";
                            clicked => { root.hide_node_type_menu(); }
                        }
                    }
                }
            }
        }
    }
}
