from __future__ import annotations

import math
import struct
from dataclasses import dataclass

from .binary import Reader, fourcc
from .errors import ParseError


@dataclass(frozen=True)
class HgmnKeyframe:
    frame: int
    value: tuple[float, ...]
    in_control: tuple[float, ...] | None = None
    out_control: tuple[float, ...] | None = None


@dataclass(frozen=True)
class HgmnChannel:
    offset: int
    opcode: int
    parameter: int
    primary: bool
    raw: bytes
    kind: str | None
    keyframes: tuple[HgmnKeyframe, ...]
    undecoded_tail: bytes


@dataclass(frozen=True)
class HgmnTarget:
    offset: int
    size: int
    object_id: int
    duration: int
    time_scale: int
    primary_channel_count: int
    event_channel_count: int
    channels: tuple[HgmnChannel, ...]

    @property
    def name(self) -> str:
        return fourcc(self.object_id)


@dataclass(frozen=True)
class Hgmn:
    flags: int
    primary_channel_count: int
    targets: tuple[HgmnTarget, ...]

    @classmethod
    def parse(cls, data: bytes, *, resource: str = "motion.hmn") -> Hgmn:
        reader = Reader(data, resource)
        if bytes(reader.require(0, 4, "HGMN signature")) != b"HGMN":
            raise ParseError("missing HGMN signature", resource=resource, offset=0)

        target_count = reader.u8(4, "target count")
        if target_count == 0:
            raise ParseError("HGMN has no targets", resource=resource, offset=4)
        flags = reader.u8(5, "flags")
        primary_channel_count = reader.u16(6, "primary channel count")
        table_size = 2 * target_count
        raw_offsets = [
            reader.u16(8 + 2 * index, f"target offset {index}")
            for index in range(target_count)
        ]
        reader.require(8, table_size, "target offset table")

        if flags & 0x80:
            starts: list[int] = []
            cursor = 0
            for delta in raw_offsets:
                if delta == 0:
                    raise ParseError(
                        "delta-encoded target offset is zero",
                        resource=resource,
                        offset=8 + 2 * len(starts),
                    )
                cursor += delta
                starts.append(cursor)
        else:
            starts = raw_offsets

        header_end = 8 + table_size
        previous = header_end - 1
        for index, start in enumerate(starts):
            if start < header_end:
                raise ParseError(
                    "target data overlaps the HGMN header",
                    resource=resource,
                    offset=8 + 2 * index,
                )
            if start <= previous:
                raise ParseError(
                    "target offsets are not strictly increasing",
                    resource=resource,
                    offset=8 + 2 * index,
                )
            if start >= len(data):
                raise ParseError(
                    "target offset is outside the resource",
                    resource=resource,
                    offset=8 + 2 * index,
                )
            previous = start

        targets = tuple(
            _parse_target(
                reader,
                start,
                starts[index + 1] if index + 1 < len(starts) else len(data),
            )
            for index, start in enumerate(starts)
        )
        decoded_primary_count = sum(target.primary_channel_count for target in targets)
        if decoded_primary_count != primary_channel_count:
            raise ParseError(
                f"primary channel count is {primary_channel_count}, "
                f"targets contain {decoded_primary_count}",
                resource=resource,
                offset=6,
            )
        return cls(
            flags=flags,
            primary_channel_count=primary_channel_count,
            targets=targets,
        )


def _parse_target(reader: Reader, start: int, end: int) -> HgmnTarget:
    size = end - start
    reader.require(start, 10, "HGMN target header")
    object_id = reader.u32(start, "target object ID")
    duration = reader.u16(start + 4, "target duration")
    time_scale = reader.i16(start + 6, "target time scale")
    primary_count = reader.u8(start + 8, "primary channel count")
    event_count = reader.u8(start + 9, "event channel count")
    channel_count = primary_count + event_count
    header_size = 10 + 2 * channel_count
    reader.require(start, header_size, "HGMN target channel table")
    relative_offsets = [
        reader.u16(start + 10 + 2 * index, f"channel offset {index}")
        for index in range(channel_count)
    ]

    previous = header_size - 1
    for index, offset in enumerate(relative_offsets):
        if offset < header_size:
            raise ParseError(
                "channel data overlaps its target header",
                resource=reader.resource,
                offset=start + 10 + 2 * index,
            )
        if offset <= previous:
            raise ParseError(
                "channel offsets are not strictly increasing",
                resource=reader.resource,
                offset=start + 10 + 2 * index,
            )
        if offset > size - 2:
            raise ParseError(
                "channel header is outside its target block",
                resource=reader.resource,
                offset=start + 10 + 2 * index,
            )
        previous = offset

    channels: list[HgmnChannel] = []
    for index, relative_offset in enumerate(relative_offsets):
        channel_end = (
            start + relative_offsets[index + 1]
            if index + 1 < channel_count
            else end
        )
        channel_start = start + relative_offset
        raw = bytes(reader.require(channel_start, channel_end - channel_start, "HGMN channel"))
        opcode = raw[0]
        parameter = raw[1]
        primary = index < primary_count
        if primary:
            kind, keyframes, undecoded_tail = _decode_channel(
                raw[2:],
                opcode,
            )
        else:
            kind, keyframes, undecoded_tail = None, (), raw[2:]
        channels.append(
            HgmnChannel(
                offset=channel_start,
                opcode=opcode,
                parameter=parameter,
                primary=primary,
                raw=raw,
                kind=kind,
                keyframes=keyframes,
                undecoded_tail=undecoded_tail,
            )
        )
    return HgmnTarget(
        offset=start,
        size=size,
        object_id=object_id,
        duration=duration,
        time_scale=time_scale,
        primary_channel_count=primary_count,
        event_channel_count=event_count,
        channels=tuple(channels),
    )


def _decode_channel(
    payload: bytes, opcode: int
) -> tuple[str | None, tuple[HgmnKeyframe, ...], bytes]:
    if opcode == 1:
        records, tail = _records(payload, 8)
        return "translation_i16", tuple(
            HgmnKeyframe(frame=frame, value=tuple(value * 0.0001 for value in values))
            for frame, values in (_unpack_record(record, "<hhhh") for record in records)
        ), tail
    if opcode == 2:
        records, tail = _records(payload, 14)
        return "translation_base_f32", tuple(
            HgmnKeyframe(
                frame=struct.unpack_from("<h", record)[0],
                value=struct.unpack_from("<fff", record, 2),
            )
            for record in records
        ), tail
    if opcode == 3:
        records, tail = _records(payload, 6)
        return "translation_scale_f32", tuple(
            HgmnKeyframe(
                frame=struct.unpack_from("<h", record)[0],
                value=(struct.unpack_from("<f", record, 2)[0],),
            )
            for record in records
        ), tail
    if opcode == 4:
        records, tail = _records(payload, 10)
        return "rotation_i16", tuple(
            HgmnKeyframe(frame=frame, value=_normalized_quaternion(values))
            for frame, values in (_unpack_record(record, "<hhhhh") for record in records)
        ), tail
    if opcode == 5:
        records, tail = _records(payload, 8)
        return "scale_i16", tuple(
            HgmnKeyframe(frame=frame, value=tuple(value / 4096.0 for value in values))
            for frame, values in (_unpack_record(record, "<hhhh") for record in records)
        ), tail
    if opcode == 12:
        keyframes, tail = _float3_records(payload)
        return "translation_f32", keyframes, tail
    if opcode == 13:
        keyframes, tail = _float3_records(payload)
        return "scale_f32", keyframes, tail
    if opcode == 14:
        records, tail = _records(payload, 18)
        return "rotation_f32", tuple(
            HgmnKeyframe(
                frame=struct.unpack_from("<h", record)[0],
                value=_normalized_quaternion(struct.unpack_from("<ffff", record, 2)),
            )
            for record in records
        ), tail
    if opcode == 16:
        records, tail = _records(payload, 38)
        return "translation_cubic_f32", tuple(
            HgmnKeyframe(
                frame=struct.unpack_from("<h", record)[0],
                value=struct.unpack_from("<fff", record, 2),
                in_control=struct.unpack_from("<fff", record, 14),
                out_control=struct.unpack_from("<fff", record, 26),
            )
            for record in records
        ), tail
    return None, (), payload


def _records(payload: bytes, stride: int) -> tuple[tuple[bytes, ...], bytes]:
    remainder = len(payload) % stride
    tail = payload[-remainder:] if remainder else b""
    complete = payload[:-remainder] if remainder else payload
    if tail and len(tail) <= 3 and not any(tail):
        tail = b""
    records = tuple(complete[index : index + stride] for index in range(0, len(complete), stride))
    return records, tail


def _unpack_record(record: bytes, fmt: str) -> tuple[int, tuple[int, ...]]:
    values = struct.unpack(fmt, record)
    return values[0], values[1:]


def _float3_records(payload: bytes) -> tuple[tuple[HgmnKeyframe, ...], bytes]:
    records, tail = _records(payload, 14)
    keyframes = tuple(
        HgmnKeyframe(
            frame=struct.unpack_from("<h", record)[0],
            value=struct.unpack_from("<fff", record, 2),
        )
        for record in records
    )
    return keyframes, tail


def _normalized_quaternion(values: tuple[int | float, ...]) -> tuple[float, ...]:
    result = (
        tuple(float(value) / 32768.0 for value in values)
        if isinstance(values[0], int)
        else tuple(float(value) for value in values)
    )
    length = math.sqrt(sum(value * value for value in result))
    if not math.isfinite(length) or length < 1e-8:
        return result
    return tuple(value / length for value in result)
