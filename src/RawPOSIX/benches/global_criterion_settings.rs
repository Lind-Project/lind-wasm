use criterion::Criterion;

pub fn get_criterion() -> Criterion {
    Criterion::default().noise_threshold(0.10)
}
