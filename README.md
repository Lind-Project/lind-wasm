# Lind

## Welcome to Lind!

Lind is a single-process sandbox that provides an option to safely execute programs. Lind executes applications using software fault isolation and a kernel microvisor to limit the potential of reaching bugs or security flaws in the application.

In Old Norse, Old High German and Old English a “lind” is a shield constructed with two layers of linden wood. Linden wood shields are lightweight, and do not split easily, an appropriate metaphor for a sandboxing system which employs two technologies.

## Getting started

A quick-way to get started is using our container via DockerHub:

```
docker pull securesystemslab/lind-wasm
docker run -it securesystemslab/lind-wasm /bin/bash
```

## Hello World!

Now let try to print `Hello world!`

```
./lindtool.sh compile_test hello
./lindtool.sh run hello
```


