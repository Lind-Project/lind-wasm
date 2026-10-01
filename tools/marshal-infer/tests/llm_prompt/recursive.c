// Direct self-recursion on a traced pointer -- the recursive call closes a
// cycle back to a function already on the current call path (the entry
// itself), which must be recorded as an Incomplete note distinct from
// ordinary call-depth truncation: even at a call-depth limit generous
// enough to "reach" the recursive call, the TRUE cumulative effect across
// unboundedly many stack frames is never representable by a bounded,
// acyclic slice.
//
// Deliberately NOT tail-recursive (the recursive call's result feeds a
// later addition, not a bare return) -- a tail-recursive version gets
// turned into an ordinary loop by this project's default -O1 compile
// profile, eliminating the actual `call` instruction the traversal needs
// to see at all.
double recursive_walk(double *x, int n) {
    if (n <= 0) return 0.0;
    x[0] += 1.0;
    double rest = recursive_walk(x + 1, n - 1);
    return rest + x[0];
}
