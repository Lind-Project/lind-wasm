/* eh_stub.c (issue #245): dummy EH frame registration.  */
void __register_frame(void* p) { (void)p; }
void __deregister_frame(void* p) { (void)p; }
