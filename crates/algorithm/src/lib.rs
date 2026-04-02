pub mod dikstra;
pub mod a_star;

pub struct Edge {
    pub node: usize,
    pub cost: usize,
}

pub trait Graph {
    type Iter<'a>: Iterator<Item=Edge> where Self: 'a;

    fn inbound(&self, node: usize) -> Self::Iter<'_>;
    fn outbound(&self, node: usize) -> Self::Iter<'_>;

    fn heuristic(&self, from: usize, to: usize) -> usize;
}


