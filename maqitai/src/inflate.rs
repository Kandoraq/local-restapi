pub fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 6 { return Err("short".into()); }
    let cmf = data[0]; let flg = data[1];
    if cmf & 0x0F != 8 { return Err("bad cmf".into()); }
    if !((cmf as u32)*256 + flg as u32).is_multiple_of(31) { return Err("bad flg".into()); }
    let pos = 2;

    if flg & 0x20 != 0 { return Err("preset dict not supported".into()); }
    let out = inflate_deflate(&data[pos..data.len()-4])?;
    Ok(out)
}

pub fn inflate_deflate(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut br = BitReader { data: input, bitpos: 0 };
    let mut out: Vec<u8> = Vec::new();
    let mut window: Vec<u8> = Vec::new();
    loop {
        let final_block = br.read_bit()?;
        let btype = br.read_bits(2)?;
        if btype == 0 {
            br.align();
            let len = br.read_u16()?;
            let nlen = br.read_u16()?;
            if len != !nlen { return Err("bad stored len".into()); }
            for _ in 0..len {
                let b = br.read_byte()?;
                out.push(b); window.push(b);
            }
        } else {
            let (lit_tree, dist_tree) = if btype == 1 {
                (fixed_lit_tree(), fixed_dist_tree())
            } else if btype == 2 {
                dynamic_trees(&mut br)?
            } else {
                return Err("reserved btype".into());
            };
            loop {
                let sym = decode_huffman(&lit_tree, &mut br)?;
                if sym < 256 {
                    let b = sym as u8;
                    out.push(b); window.push(b);
                } else if sym == 256 {
                    break;
                } else {
                    let len = decode_length(sym, &mut br)?;
                    let dist_sym = decode_huffman(&dist_tree, &mut br)?;
                    let dist = decode_distance(dist_sym, &mut br)?;
                    for _ in 0..len {
                        let idx = window.len() - dist;
                        let b = window[idx];
                        out.push(b); window.push(b);
                    }
                }
            }
        }
        if final_block == 1 { break; }
    }
    Ok(out)
}

pub struct BitReader<'a> { data: &'a [u8], bitpos: usize }

impl<'a> BitReader<'a> {
    fn read_bit(&mut self) -> Result<u8, String> {
        if self.bitpos / 8 >= self.data.len() { return Err("eof".into()); }
        let b = (self.data[self.bitpos/8] >> (self.bitpos % 8)) & 1;
        self.bitpos += 1;
        Ok(b)
    }
    fn read_bits(&mut self, n: usize) -> Result<usize, String> {
        let mut v = 0usize;
        for i in 0..n {
            let b = self.read_bit()?;
            v |= (b as usize) << i;
        }
        Ok(v)
    }
    fn align(&mut self) {
        if !self.bitpos.is_multiple_of(8) { self.bitpos += 8 - (self.bitpos % 8); }
    }
    fn read_u16(&mut self) -> Result<u16, String> {
        if self.bitpos/8 + 2 > self.data.len() { return Err("eof".into()); }
        let i = self.bitpos/8;
        let v = u16::from_le_bytes([self.data[i], self.data[i+1]]);
        self.bitpos += 16;
        Ok(v)
    }
    fn read_byte(&mut self) -> Result<u8, String> {
        if self.bitpos/8 >= self.data.len() { return Err("eof".into()); }
        let b = self.data[self.bitpos/8];
        self.bitpos += 8;
        Ok(b)
    }
}

pub enum Node { Leaf(u16), Branch(Box<Node>, Box<Node>) }

pub fn decode_huffman(tree: &Node, br: &mut BitReader) -> Result<u16, String> {
    let mut node = tree;
    loop {
        match node {
            Node::Leaf(s) => return Ok(*s),
            Node::Branch(l,r) => {
                let b = br.read_bit()?;
                node = if b == 0 { l } else { r };
            }
        }
    }
}

pub fn build_tree(lengths: &[u8]) -> Node {
    let max = *lengths.iter().max().unwrap_or(&0) as usize;
    if max == 0 { return Node::Leaf(256); }
    let mut counts = vec![0usize; max+1];
    for &l in lengths { if l > 0 { counts[l as usize] += 1; } }
    let mut next_code = vec![0usize; max+2];
    let mut code = 0usize;
    for bits in 1..=max {
        code = (code + counts[bits-1]) << 1;
        next_code[bits] = code;
    }
    let mut codes = vec![0usize; lengths.len()];
    for (i,&l) in lengths.iter().enumerate() {
        if l > 0 {
            let bits = l as usize;
            codes[i] = next_code[bits];
            next_code[bits] += 1;
        }
    }
    let mut root = Node::Branch(Box::new(Node::Leaf(0)), Box::new(Node::Leaf(0)));
    for (i,&l) in lengths.iter().enumerate() {
        if l == 0 { continue; }
        let bits = l as usize;
        insert_node(&mut root, codes[i], bits, i as u16);
    }
    root
}

pub fn insert_node(node: &mut Node, code: usize, bits: usize, symbol: u16) {
    if bits == 0 {
        *node = Node::Leaf(symbol);
        return;
    }
    if let Node::Leaf(_) = node {
        *node = Node::Branch(Box::new(Node::Leaf(0)), Box::new(Node::Leaf(0)));
    }
    if let Node::Branch(l, r) = node {
        let bit = (code >> (bits - 1)) & 1;
        if bit == 0 {
            insert_node(l, code, bits - 1, symbol);
        } else {
            insert_node(r, code, bits - 1, symbol);
        }
    }
}

pub fn fixed_lit_tree() -> Node {
    let mut lengths = vec![0u8; 288];
    for item in lengths.iter_mut().take(144) { *item = 8; }
    for item in lengths.iter_mut().take(256).skip(144) { *item = 9; }
    for item in lengths.iter_mut().take(280).skip(256) { *item = 7; }
    for item in lengths.iter_mut().take(288).skip(280) { *item = 8; }
    build_tree(&lengths)
}

pub fn fixed_dist_tree() -> Node {
    let lengths = vec![5u8; 32];
    build_tree(&lengths)
}

pub fn dynamic_trees(br: &mut BitReader) -> Result<(Node,Node), String> {
    let hlit = br.read_bits(5)? + 257;
    let hdist = br.read_bits(5)? + 1;
    let hclen = br.read_bits(4)? + 4;
    let order = [16usize,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];
    let mut cl_lengths = vec![0u8; 19];
    for i in 0..hclen {
        cl_lengths[order[i]] = br.read_bits(3)? as u8;
    }
    let cl_tree = build_tree(&cl_lengths);
    let total = hlit + hdist;
    let mut lengths = vec![0u8; total];
    let mut i = 0;
    while i < total {
        let sym = decode_huffman(&cl_tree, br)?;
        match sym {
            0..=15 => { lengths[i] = sym as u8; i += 1; }
            16 => {
                let rep = br.read_bits(2)? + 3;
                let prev = if i > 0 { lengths[i-1] } else { 0 };
                for _ in 0..rep { if i < total { lengths[i] = prev; i += 1; } }
            }
            17 => {
                let rep = br.read_bits(3)? + 3;
                for _ in 0..rep { if i < total { lengths[i] = 0; i += 1; } }
            }
            18 => {
                let rep = br.read_bits(7)? + 11;
                for _ in 0..rep { if i < total { lengths[i] = 0; i += 1; } }
            }
            _ => return Err("bad cl sym".into()),
        }
    }
    let lit_lengths = &lengths[..hlit];
    let dist_lengths = &lengths[hlit..];
    Ok((build_tree(lit_lengths), build_tree(dist_lengths)))
}

pub fn decode_length(sym: u16, br: &mut BitReader) -> Result<usize, String> {
    let base = [3usize,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
    let extra = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0];
    let idx = (sym as usize) - 257;
    if idx >= base.len() { return Err("bad length sym".into()); }
    let mut v = base[idx];
    if extra[idx] > 0 { v += br.read_bits(extra[idx])?; }
    Ok(v)
}

pub fn decode_distance(sym: u16, br: &mut BitReader) -> Result<usize, String> {
    let base = [1usize,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
    let extra = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13];
    let idx = sym as usize;
    if idx >= base.len() { return Err("bad dist sym".into()); }
    let mut v = base[idx];
    if extra[idx] > 0 { v += br.read_bits(extra[idx])?; }
    Ok(v)
}

pub fn inflate_gzip(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 10 { return Err("short gzip".into()); }
    if data[0] != 0x1f || data[1] != 0x8b { return Err("bad gzip magic".into()); }
    if data[2] != 8 { return Err("bad gzip method".into()); }
    let flags = data[3];
    let mut pos = 10usize;
    if flags & 0x04 != 0 {
        if pos + 2 > data.len() { return Err("short xlen".into()); }
        let xlen = u16::from_le_bytes([data[pos], data[pos+1]]) as usize;
        pos += 2 + xlen;
    }
    if flags & 0x08 != 0 {
        while pos < data.len() && data[pos] != 0 { pos += 1; }
        pos += 1;
    }
    if flags & 0x10 != 0 {
        while pos < data.len() && data[pos] != 0 { pos += 1; }
        pos += 1;
    }
    if flags & 0x02 != 0 { pos += 2; }
    if pos + 8 > data.len() { return Err("short gzip trailer".into()); }
    let compressed = &data[pos..data.len()-8];
    inflate_deflate(compressed)
}
