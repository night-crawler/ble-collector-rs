pub(crate) trait Evaluate<S, R> {
    fn evaluate(&self, source: S) -> R;
}
