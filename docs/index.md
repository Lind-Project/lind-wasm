---
id: Overview
hide:
  - toc
---

<section class="lw-hero">
  <div class="lw-hero__content">
    <div class="lw-brand">
      <img src="images/lindv1.png" alt="Lind-Wasm logo">
      <span>Lind-Wasm</span>
    </div>
    <h1>POSIX-style isolation, rebuilt for WebAssembly.</h1>
    <p class="lw-hero__lead">
      Run mutually untrusted Linux-style workloads as isolated WebAssembly cages
      inside one unprivileged process, with programmable syscall interposition
      and a small trusted runtime.
    </p>
    <div class="lw-actions">
      <a class="lw-button lw-button--primary" href="getting-started/">Get started</a>
      <a class="lw-button lw-button--secondary" href="internal/">Explore internals</a>
    </div>
  </div>
  <div class="lw-runtime-panel" aria-label="Lind-Wasm runtime flow">
    <div class="lw-panel-top">
      <span></span>
      <span></span>
      <span></span>
      <code>lind-wasm runtime</code>
    </div>
    <div class="lw-stack">
      <div class="lw-grid">
        <div class="lw-node lw-node--app">
          <strong>Application cage</strong>
          <span>Wasm + lind-glibc</span>
        </div>
        <div class="lw-node lw-node--grate">
          <strong>Grate</strong>
          <span>policy cage</span>
        </div>
      </div>
      <div class="lw-route">
        <span>syscalls through 3i</span>
      </div>
      <div class="lw-node lw-node--threei">
        <strong>3i routing table</strong>
        <span>lookup, interpose, delegate</span>
      </div>
      <div class="lw-route">
        <span>host calls</span>
      </div>
      <div class="lw-node lw-node--rawposix">
        <strong>RawPOSIX</strong>
        <span>trusted host boundary</span>
      </div>
    </div>
  </div>
</section>

<section class="lw-strip" aria-label="Core properties">
  <div>
    <strong>Single host process</strong>
    <span>Many isolated cages share one unprivileged runtime.</span>
  </div>
  <div>
    <strong>POSIX-oriented</strong>
    <span>Applications use a modified glibc and familiar process semantics.</span>
  </div>
  <div>
    <strong>Programmable mediation</strong>
    <span>3i can inspect, redirect, or handle system calls.</span>
  </div>
</section>

## Built For Sandboxed Systems Research

<div class="lw-feature-grid">
  <article>
    <h3>Run untrusted programs</h3>
    <p>
      Compile C/POSIX workloads to WebAssembly and execute them in cages with
      isolated memory, control flow, and syscall routing state.
    </p>
  </article>
  <article>
    <h3>Interpose without kernel changes</h3>
    <p>
      Route calls through 3i to userspace grates or RawPOSIX, enabling policy
      and system services without privileged execution.
    </p>
  </article>
  <article>
    <h3>Study runtime boundaries</h3>
    <p>
      Explore the division between Wasmtime, lind-glibc, 3i, RawPOSIX, and
      multiprocess support through focused internal documentation.
    </p>
  </article>
</div>

## How It Fits Together

<div class="lw-system">
  <div class="lw-system__copy">
    <p>
      Lind-Wasm realizes Lind with WebAssembly software fault isolation and a
      compact trusted runtime. Applications issue system calls through 3i; those
      calls can be routed to grates at the cage layer or passed down to RawPOSIX
      for host interaction.
    </p>
    <div class="lw-link-list">
      <a href="internal/3i/">3i syscall routing</a>
      <a href="internal/grates/">Grates</a>
      <a href="internal/rawposix/">RawPOSIX</a>
      <a href="internal/wasmtime/">Wasmtime integration</a>
    </div>
  </div>
  <figure class="lw-system__visual">
    <img src="images/doc-images/syscall_flow_diagram.svg" alt="Lind-Wasm syscall routing flow">
  </figure>
</div>

## Runtime Layers

<div class="lw-layer-list">
  <a href="internal/libc/">
    <span>01</span>
    <strong>lind-glibc</strong>
    <em>POSIX-facing libc adapted for WebAssembly and 3i calls.</em>
  </a>
  <a href="internal/3i/">
    <span>02</span>
    <strong>3i</strong>
    <em>Per-cage handler tables for syscall lookup, delegation, and interposition.</em>
  </a>
  <a href="internal/wasmtime/">
    <span>03</span>
    <strong>Wasmtime</strong>
    <em>Execution engine and embedding layer for isolated cages.</em>
  </a>
  <a href="internal/rawposix/">
    <span>04</span>
    <strong>RawPOSIX</strong>
    <em>Trusted runtime services that mediate access to host kernel behavior.</em>
  </a>
</div>

## Start Here

<div class="lw-start-grid">
  <a href="getting-started/">
    <strong>Getting started</strong>
    <span>Set up the project and run the first Lind-Wasm workflow.</span>
  </a>
  <a href="contribute/testing/">
    <strong>Testing</strong>
    <span>Run unit, integration, and end-to-end test paths.</span>
  </a>
  <a href="contribute/">
    <strong>Contributing</strong>
    <span>Follow the development, style, and pipeline conventions.</span>
  </a>
</div>
