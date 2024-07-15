import pathlib
import operator

import pytest

try:
    from yaml import CSafeLoader as SafeLoader, load
except ImportError:
    from yaml import SafeLoader, load  # type: ignore

import ua_parser_rs


CORE_DIR = pathlib.Path(__file__).resolve().parents[2] / "ua-parser" / "uap-core"


MISSING_UA = {
    "family": "Other",
    "major": None,
    "minor": None,
    "patch": None,
    "patch_minor": None,
}
get_reference = operator.itemgetter(*MISSING_UA)
get_result = operator.attrgetter(*MISSING_UA)


@pytest.mark.parametrize(
    "test_file",
    [
        CORE_DIR / "tests" / "test_os.yaml",
        CORE_DIR / "test_resources" / "additional_os_tests.yaml",
    ],
    ids=operator.attrgetter("name"),
)
def test_os(test_file: pathlib.Path) -> None:
    with (CORE_DIR / "regexes.yaml").open("rb") as f:
        contents = load(f, Loader=SafeLoader)

    parser = ua_parser_rs.OSExtractor(
        (
            t["regex"],
            t.get("os_replacement"),
            t.get("os_v1_replacement"),
            t.get("os_v2_replacement"),
            t.get("os_v3_replacement"),
            t.get("os_v4_replacement"),
        )
        for t in contents["os_parsers"]
    )

    with test_file.open("rb") as f:
        contents = load(f, Loader=SafeLoader)

    for test_case in contents["test_cases"]:
        r = parser.extract(test_case["user_agent_string"])
        if r:
            result = get_result(r)
        else:
            result = get_reference(MISSING_UA)

        print(test_case)
        assert result == get_reference(test_case)
