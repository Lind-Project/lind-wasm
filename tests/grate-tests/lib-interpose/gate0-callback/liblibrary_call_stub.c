// Dummy backing implementation for `library_call`, staged as the preloaded
// "env=/lib/..." module so dylink import resolution has something to
// resolve against. Never actually executes under strict interposition --
// the grate's registered V2 handler intercepts every call before this body
// would run; a non-trivial body here would make a silent fallback-to-real-
// library bug look identical to a correct interposed call.
void library_call(void (*callback)(int)) {
    (void)callback;
}
