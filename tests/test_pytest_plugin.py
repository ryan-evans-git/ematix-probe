"""S-9.1 — pytest plugin scaffolding.

The plugin discovers `DataProbe` instances at module top level and
exposes each one as a pytest collection item, so users can write
their probes alongside their pytest tests and `pytest` picks them
up automatically.

This test relies on the built-in `pytester` fixture. Running pytest
on a synthesized test file proves end-to-end that:
1. The plugin module is importable as `ematix_probe.pytest_plugin`.
2. Loading it via `pytest_plugins = [...]` in a conftest gives
   pytest the collection hook.
3. A `@probe.data` instance at module top level becomes a collected
   pytest item (visible to `--collect-only`).
"""

import pytest

pytest_plugins = ["pytester"]


def test_plugin_module_is_importable():
    import ematix_probe.pytest_plugin  # noqa: F401


def test_data_probe_is_collected_as_pytest_item(pytester):
    pytester.makepyfile(
        """
        from ematix_probe import probe, source

        @probe.data(
            source=source.postgres("postgres://localhost/x"),
            table="t",
        )
        def my_quality_check(t):
            t.column("id").not_null()
        """
    )
    pytester.makeconftest(
        """
        pytest_plugins = ["ematix_probe.pytest_plugin"]
        """
    )
    result = pytester.runpytest("--collect-only", "-q")
    # Plugin must surface the DataProbe as a collection item with
    # the original function's name visible.
    result.stdout.fnmatch_lines(["*my_quality_check*"])
    assert result.ret == pytest.ExitCode.OK


def test_non_data_probe_module_attrs_are_left_alone(pytester):
    # Plain functions / values must still be collected normally
    # (or ignored) — the plugin only intercepts DataProbe
    # instances, never other attributes.
    pytester.makepyfile(
        """
        from ematix_probe import probe, source

        SOME_CONSTANT = 42

        def test_normal_function():
            assert SOME_CONSTANT == 42

        @probe.data(
            source=source.postgres("postgres://localhost/x"),
            table="t",
        )
        def probe_check(t):
            t.column("id").not_null()
        """
    )
    pytester.makeconftest(
        """
        pytest_plugins = ["ematix_probe.pytest_plugin"]
        """
    )
    result = pytester.runpytest("--collect-only", "-q")
    # Both the plain test and the DataProbe should be collected.
    result.stdout.fnmatch_lines(["*test_normal_function*"])
    result.stdout.fnmatch_lines(["*probe_check*"])
    assert result.ret == pytest.ExitCode.OK
