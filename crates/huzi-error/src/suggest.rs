//! "did you mean" 修复建议:用 Levenshtein 编辑距离在候选名中找最接近项。

/// 两个字符串之间的 Levenshtein 编辑距离(插入/删除/替换,代价均为 1)。
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// 建议距离上限:名称越长允许的容错越多,但封顶 3。
const MAX_DISTANCE: usize = 3;

/// 在候选名中寻找与 `name` 最接近的一项(大小写不敏感的完全匹配优先),
/// 返回可直接追加到错误信息的提示文本,如 ``Some("did you mean `count`?")``。
pub fn did_you_mean<'a, I>(name: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        if candidate == name {
            continue;
        }
        let distance = if candidate.eq_ignore_ascii_case(name) {
            0
        } else {
            levenshtein(name, candidate)
        };
        if distance > MAX_DISTANCE {
            continue;
        }
        if best.map_or(true, |(best_distance, _)| distance < best_distance) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, candidate)| format!("did you mean `{}`?", candidate))
}
