//! A CALL'S URL PATTERN AGAINST AN ENDPOINT'S ROUTE: both split into lowercase segments, a parameter segment
//! standing for any one, an unknown base (a leading `*`) for any number of leading ones.

use super::calls::Site;
use super::endpoints::Endpoint;

/// An endpoint's route as stored: lowercase segments, a placeholder `{}` (`{}?` when optional).
pub(super) fn endpoint_route(template: &str) -> String {
    let lower = template.to_lowercase();
    let segments: Vec<String> = lower
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| match s.contains('{') {
            true if s.ends_with("?}") => "{}?".to_string(),
            true => "{}".to_string(),
            false => s.to_string(),
        })
        .collect();
    segments.join("/")
}

/// A call's url as segments: a scheme and host dropped, an unknown base written `*api/x` read as `*/api/x`.
pub(super) fn call_segments(url: &str) -> Vec<String> {
    let lower = url.to_lowercase();
    let path = match lower.strip_prefix("http://").or_else(|| lower.strip_prefix("https://")) {
        Some(rest) => rest.find('/').map(|i| rest[i..].to_string()).unwrap_or_default(),
        None => lower,
    };
    let path = if path.len() > 1 && path.starts_with('*') && !path.starts_with("*/") { format!("*/{}", &path[1..]) } else { path };
    path.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// The routes an endpoint answers to: with and without each optional segment.
fn variants(route: &str) -> Vec<Vec<String>> {
    let mut all: Vec<Vec<String>> = vec![vec![]];
    for seg in route.split('/').filter(|s| !s.is_empty()) {
        if seg == "{}?" {
            let mut with = all.clone();
            with.iter_mut().for_each(|v| v.push("{}".to_string()));
            all.extend(with);
        } else {
            all.iter_mut().for_each(|v| v.push(seg.to_string()));
        }
        // A route with a dozen optional parts is no route anyone calls by every spelling.
        all.truncate(64);
    }
    all
}

fn segment(call: &str, endpoint: &str) -> bool {
    endpoint == "{}" || endpoint == "*" || call == "{}" || call.contains('*') || call == endpoint
}

/// How many of the call's segments name the endpoint's in so many words - None when the path does not match at all.
/// A parameter or an unknown part matches anything and proves nothing, so it does not count.
fn literal_hits(call: &[String], endpoint: &[String]) -> Option<usize> {
    let pair = |c: &[String], e: &[String]| {
        (c.len() == e.len() && c.iter().zip(e).all(|(c, e)| segment(c, e)))
            .then(|| c.iter().zip(e).filter(|(c, e)| *c == *e && !e.contains('{') && !c.contains('*') && *c != "{}").count())
    };
    match call.split_first() {
        // THE UNKNOWN BASE stands for any number of leading segments, none included.
        Some((first, rest)) if first == "*" && !rest.is_empty() => endpoint.len().checked_sub(rest.len()).and_then(|k| pair(rest, &endpoint[k..])),
        _ => pair(call, endpoint),
    }
}

#[cfg(test)]
fn same_path(call: &[String], endpoint: &[String]) -> bool {
    literal_hits(call, endpoint).is_some()
}

fn verbs(call: &str, endpoint: &str) -> bool {
    call == endpoint || call == "ANY" || endpoint == "ANY"
}

/// For each call, the indexes of every endpoint it reaches.
pub(super) fn link(endpoints: &[Endpoint], calls: &[Site]) -> Vec<Vec<usize>> {
    let routes: Vec<Vec<Vec<String>>> = endpoints.iter().map(|e| variants(&e.route)).collect();
    calls
        .iter()
        .map(|call| {
            let segments = call_segments(&call.url);
            // A url that is nothing but unknown would reach every endpoint there is, which says nothing.
            if segments.iter().all(|s| s == "*") {
                return vec![];
            }
            // THE ENDPOINTS THE URL NAMES BEST: `*/enabled-features` matched every route ending in a parameter, and a
            // parameter proves nothing. Only the endpoints with the most literal segments in common are kept, and a
            // match with none in common is none at all.
            let scored: Vec<(usize, usize)> = (0..endpoints.len())
                .filter(|&i| verbs(&call.verb, &endpoints[i].verb))
                .filter_map(|i| routes[i].iter().filter_map(|r| literal_hits(&segments, r)).max().map(|hits| (i, hits)))
                .collect();
            let best = scored.iter().map(|&(_, hits)| hits).max().unwrap_or(0);
            if best == 0 {
                return vec![];
            }
            scored.into_iter().filter(|&(_, hits)| hits == best).map(|(i, _)| i).collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(s: &str) -> Vec<String> {
        call_segments(s)
    }

    fn reaches(call: &str, route: &str) -> bool {
        variants(&endpoint_route(route)).iter().any(|r| same_path(&segs(call), r))
    }

    #[test]
    fn an_unknown_base_stands_for_leading_segments() {
        assert!(reaches("*api/calculations/save", "api/Calculations/Save"));
        assert!(reaches("*/save", "api/calculations/save"));
        assert!(!reaches("*/other", "api/calculations/save"));
        assert!(reaches("*save", "save"));
    }

    #[test]
    fn a_parameter_matches_one_segment() {
        assert!(reaches("*/api/calc/{}", "api/calc/{id}"));
        assert!(!reaches("*/api/calc/{}/x", "api/calc/{id}"));
        assert!(reaches("/api/calc/a*", "api/calc/{id:int}"));
        assert!(reaches("api/calc", "api/calc/{id?}"));
        assert!(reaches("api/calc/5", "api/calc/{id?}"));
    }

    #[test]
    fn a_host_is_dropped_and_the_case_is_not_compared() {
        assert_eq!(segs("https://Host.example/Api/X"), vec!["api", "x"]);
        assert!(reaches("http://h/API/x", "api/X"));
    }

    fn linked(call: &str, routes: &[&str]) -> Vec<usize> {
        let endpoints: Vec<Endpoint> = routes.iter().map(|r| Endpoint { file: String::new(), cls: String::new(), method: String::new(), line: 0, verb: "GET".into(), template: r.to_string(), route: endpoint_route(r) }).collect();
        link(&endpoints, &[Site { file: String::new(), call: String::new(), member: String::new(), line: 0, verb: "GET".into(), url: call.to_string(), via: String::new() }]).remove(0)
    }

    #[test]
    fn the_endpoints_the_url_names_best_win_and_a_parameter_alone_proves_nothing() {
        // `{id}` takes `enabled-features` as a value, but `features/enabled-features` names it.
        assert_eq!(linked("*/enabled-features", &["api/features/enabled-features", "api/orders/{id}", "api/x/{id}"]), vec![0]);
        // only parameters in common: no link at all.
        assert_eq!(linked("*/enabled-features", &["api/orders/{id}"]), Vec::<usize>::new());
        // two routes that both name every literal stay ambiguous.
        assert_eq!(linked("*/calc/save", &["api/calc/save", "v2/calc/save"]), vec![0, 1]);
    }

    #[test]
    fn no_base_no_equal_counts() {
        assert!(!reaches("api/x", "api/x/y"));
        assert!(!reaches("a/api/x", "api/x"));
    }
}
