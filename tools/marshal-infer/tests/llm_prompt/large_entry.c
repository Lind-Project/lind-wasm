// The entry function ALONE has enough instructions to exceed a small
// configured instruction budget -- exercises the "entry instruction-budget
// policy" documented in LlmPrompt.cpp: the entry must still be included
// (a prompt missing the very function being classified would be useless),
// but slice_complete must be false with an explicit note naming the
// entry itself as the cause, not silently claim completeness.
double large_entry(double *x, int n) {
    double s0 = x[0], s1 = x[1], s2 = x[2], s3 = x[3];
    s0 += 1.0; s1 += 2.0; s2 += 3.0; s3 += 4.0;
    s0 *= 1.1; s1 *= 1.2; s2 *= 1.3; s3 *= 1.4;
    s0 -= 0.1; s1 -= 0.2; s2 -= 0.3; s3 -= 0.4;
    s0 /= 1.5; s1 /= 1.6; s2 /= 1.7; s3 /= 1.8;
    s0 += s1; s2 += s3; s0 += s2;
    s0 += 1.0; s1 += 2.0; s2 += 3.0; s3 += 4.0;
    s0 *= 1.1; s1 *= 1.2; s2 *= 1.3; s3 *= 1.4;
    s0 -= 0.1; s1 -= 0.2; s2 -= 0.3; s3 -= 0.4;
    s0 /= 1.5; s1 /= 1.6; s2 /= 1.7; s3 /= 1.8;
    s0 += s1; s2 += s3; s0 += s2;
    s0 += 1.0; s1 += 2.0; s2 += 3.0; s3 += 4.0;
    s0 *= 1.1; s1 *= 1.2; s2 *= 1.3; s3 *= 1.4;
    s0 -= 0.1; s1 -= 0.2; s2 -= 0.3; s3 -= 0.4;
    s0 /= 1.5; s1 /= 1.6; s2 /= 1.7; s3 /= 1.8;
    s0 += s1; s2 += s3; s0 += s2;
    return s0 + (double)n;
}
