pub struct Cursor<'a> {
    input: &'a [u8],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    #[inline(always)]
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    #[inline(always)]
    pub fn remaining(&self) -> &'a [u8] {
        if self.pos >= self.input.len() {
            &[]
        } else {
            &self.input[self.pos..]
        }
    }

    #[inline(always)]
    pub fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    /// 极速跳过空白字符
    pub fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\n' | b'\t' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    /// 尝试匹配前缀，如果不匹配则不移动游标
    pub fn matches(&self, pattern: &[u8]) -> bool {
        self.remaining().starts_with(pattern)
    }

    /// 也是推测解析的核心：寻找下一个最近的锚点
    /// 返回找到的 Key 和它开始的位置
    pub fn find_next_anchor<'b>(&self, anchors: &'b [Vec<u8>]) -> Option<(&'b [u8], usize)> {
        // 这是一个 O(N*M) 的朴素实现。
        // 生产环境：如果 anchors 很多，这里必须上 Aho-Corasick 自动机。
        // 但对于 LLM 场景（通常 < 20 个字段），朴素查找通常跑得过构建自动机的开销。

        let input = self.remaining();
        // 限制向前看 512 字节，避免扫描整个几 MB 的文档
        let limit = std::cmp::min(input.len(), 512);

        for i in 0..limit {
            for anchor in anchors {
                if input[i..].starts_with(anchor) {
                    return Some((anchor, self.pos + i));
                }
            }
        }
        None
    }
}
