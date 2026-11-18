"""Validated, non-evaluating JSON command execution."""

import inspect
import types
import typing
from typing import Any

from .model import DemoError, World, require


COMMANDS = (
    "init", "create", "launch", "map", "protect", "unmap", "translate", "abi",
    "convert", "write", "read", "copy", "probe", "set_iovec", "fork", "clone", "unwind",
    "signal", "register", "unregister", "call", "return_handler", "exec", "apply",
    "exit_thread", "terminate", "wait", "sigreturn", "tick", "state", "check",
)


def matches(value, annotation):
    if annotation is Any:
        return True
    origin = typing.get_origin(annotation)
    if origin in (typing.Union, types.UnionType):
        return any(matches(value, a) for a in typing.get_args(annotation))
    if origin is list:
        return isinstance(value, list) and all(matches(v, typing.get_args(annotation)[0]) for v in value)
    if origin is dict:
        return isinstance(value, dict) and all(
            matches(k, typing.get_args(annotation)[0]) and matches(v, typing.get_args(annotation)[1])
            for k, v in value.items()
        )
    return type(value) is annotation


class Runner:
    def __init__(self):
        self.world = World()
        self.variables: dict[str, Any] = {}

    def resolve(self, value):
        if isinstance(value, str) and value.startswith("$"):
            name = value[1:]
            require(name in self.variables, "INPUT", f"unknown saved value {name!r}")
            return self.variables[name]
        if isinstance(value, list):
            return [self.resolve(v) for v in value]
        if isinstance(value, dict):
            return {k: self.resolve(v) for k, v in value.items()}
        return value

    def execute(self, step):
        require(isinstance(step, dict), "INPUT", "step must be a JSON object")
        command = step.get("op")
        require(isinstance(command, str) and command in COMMANDS, "INPUT", f"unknown command {command!r}")
        expected = step.get("expect_error")
        require(expected is None or isinstance(expected, str), "INPUT", "expect_error must be a code")
        save = step.get("save")
        require(save is None or isinstance(save, str), "INPUT", "save must be a variable name")
        method = getattr(self.world, command)
        arguments = self.resolve({k: v for k, v in step.items() if k not in ("op", "save", "expect_error")})
        try:
            inspect.signature(method).bind(**arguments)
        except TypeError as error:
            raise DemoError("INPUT", str(error)) from error
        hints = typing.get_type_hints(method)
        for key, value in arguments.items():
            require(matches(value, hints.get(key, Any)), "INPUT",
                    f"{command}.{key} has wrong type")
        self.world.emit("sequence", "command", command=command, arguments=arguments)
        try:
            result = method(**arguments)
        except ValueError as error:
            # Only user-provided hex decoding raises ValueError in model commands.
            raise DemoError("INPUT", str(error)) from error
        except DemoError as error:
            self.world.emit("sequence", "error", code=error.code, message=str(error),
                            expected=expected == error.code)
            self.world.invariant()
            if expected != error.code:
                raise
            result = dict(error=error.code, message=str(error))
        else:
            require(expected is None, "ASSERT", f"expected {expected}, but {command} succeeded")
        self.world.invariant()
        if command != "check":
            self.world.last = result
        if save is not None:
            self.variables[save] = result
        self.world.emit("sequence", "result", command=command, result=result)
        return result

    def run(self, steps):
        require(isinstance(steps, list), "INPUT", "sequence must be a JSON array")
        for index, step in enumerate(steps, 1):
            try:
                self.execute(step)
            except DemoError as error:
                raise DemoError(error.code, f"step {index}: {error}") from error
        return self.world
