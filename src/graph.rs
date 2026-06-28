use serde::Serialize;
use serde_json;

use crate::types::ClaimedProfile;

#[derive(Debug, Clone, Serialize)]
pub struct AccountGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub site_name: String,
    pub url: String,
    pub display_name: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub label: String,
}

pub fn build_account_graph(profiles: &[ClaimedProfile]) -> AccountGraph {
    let nodes: Vec<GraphNode> = profiles.iter().map(|p| GraphNode {
        id: p.site_name.clone(),
        site_name: p.site_name.clone(),
        url: p.site_url.clone(),
        display_name: p.details.display_name.clone(),
        confidence: 1.0,
    }).collect();

    let mut edges = Vec::new();

    for profile in profiles {
        for url in &profile.details.linked_urls {
            if let Some(target) = profiles.iter().find(|p| {
                p.site_name != profile.site_name && url.contains(&p.site_url)
            }) {
                edges.push(GraphEdge {
                    from: profile.site_name.clone(),
                    to: target.site_name.clone(),
                    label: "link in bio".to_string(),
                });
            }
        }
    }

    AccountGraph { nodes, edges }
}

pub fn to_dot(graph: &AccountGraph) -> String {
    let mut dot = String::from("digraph raven {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [shape=box, style=filled, fillcolor=lightblue];\n\n");

    for node in &graph.nodes {
        let label = node.display_name.as_deref().unwrap_or(&node.site_name);
        dot.push_str(&format!(
            "  \"{}\" [label=\"{}\\n{}\", URL=\"{}\"];\n",
            node.id, node.site_name, label, node.url
        ));
    }

    dot.push('\n');

    for edge in &graph.edges {
        dot.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            edge.from, edge.to, edge.label
        ));
    }

    dot.push('}');
    dot
}

pub fn to_d3_json(graph: &AccountGraph) -> serde_json::Value {
    serde_json::json!({
        "nodes": graph.nodes.iter().map(|n| serde_json::json!({
            "id": n.id,
            "site": n.site_name,
            "url": n.url,
            "name": n.display_name,
        })).collect::<Vec<_>>(),
        "links": graph.edges.iter().map(|e| serde_json::json!({
            "source": e.from,
            "target": e.to,
            "label": e.label,
        })).collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProfileDetails;

    fn make_profile(site: &str, url_suffix: &str, linked_urls: Vec<&str>) -> ClaimedProfile {
        ClaimedProfile {
            site_name: site.to_string(),
            site_url: format!("https://{}.com{}", site.to_lowercase(), url_suffix),
            username: "test".to_string(),
            details: ProfileDetails {
                display_name: Some(site.to_string()),
                linked_urls: linked_urls.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            avatar_phash: None,
        }
    }

    #[test]
    fn test_build_graph_empty() {
        let g = build_account_graph(&[]);
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_build_graph_no_edges() {
        let p = make_profile("GitHub", "/user", vec![]);
        let g = build_account_graph(&[p]);
        assert_eq!(g.nodes.len(), 1);
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_build_graph_with_cross_link() {
        let p1 = make_profile("GitHub", "/user", vec!["https://reddit.com/user/test"]);
        let p2 = make_profile("Reddit", "/user", vec![]);
        let g = build_account_graph(&[p1, p2]);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].from, "GitHub");
        assert_eq!(g.edges[0].to, "Reddit");
    }

    #[test]
    fn test_to_dot_contains_nodes() {
        let p = make_profile("GitHub", "/user", vec![]);
        let g = build_account_graph(&[p]);
        let dot = to_dot(&g);
        assert!(dot.contains("GitHub"));
        assert!(dot.contains("digraph raven"));
    }

    #[test]
    fn test_to_d3_json() {
        let p = make_profile("GitHub", "/user", vec![]);
        let g = build_account_graph(&[p]);
        let json = to_d3_json(&g);
        assert_eq!(json["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(json["links"].as_array().unwrap().len(), 0);
    }
}
