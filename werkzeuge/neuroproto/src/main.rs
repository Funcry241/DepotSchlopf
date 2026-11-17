//! Minimaler Prototyp wie Neuro2026: 3 lineare Schichten, SiLU, CPU-Backend Flex, ein Vorhersageschritt.
use burn::backend::Flex;
use burn::nn::{Linear, LinearConfig};
use burn::prelude::*;
use burn::tensor::activation::silu;

#[derive(Module, Debug)]
struct Netz<B: Backend> { a: Linear<B>, b: Linear<B>, c: Linear<B> }

fn main() {
    type B = Flex;
    let d = Default::default();
    let n: Netz<B> = Netz {
        a: LinearConfig::new(8, 48).init(&d),
        b: LinearConfig::new(48, 48).init(&d),
        c: LinearConfig::new(48, 1).init(&d),
    };
    let x = Tensor::<B, 2>::ones([1, 8], &d);
    let y = n.c.forward(silu(n.b.forward(silu(n.a.forward(x)))));
    println!("score={}", y.into_scalar());
}
