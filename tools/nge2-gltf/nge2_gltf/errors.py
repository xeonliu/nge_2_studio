from __future__ import annotations


class ConversionError(Exception):
    """Base error carrying enough source context for a conversion report."""

    def __init__(self, message: str, *, resource: str | None = None, offset: int | None = None):
        super().__init__(message)
        self.message = message
        self.resource = resource
        self.offset = offset

    def __str__(self) -> str:
        location = self.resource or "input"
        if self.offset is not None:
            location += f"@0x{self.offset:X}"
        return f"{location}: {self.message}"

    def as_report(self) -> dict[str, str | int]:
        result: dict[str, str | int] = {"message": self.message}
        if self.resource is not None:
            result["resource"] = self.resource
        if self.offset is not None:
            result["offset"] = self.offset
            result["offsetHex"] = f"0x{self.offset:X}"
        return result


class ParseError(ConversionError):
    """Malformed or inconsistent source data."""


class UnsupportedFeature(ConversionError):
    """Well-formed source data using functionality not decoded yet."""
