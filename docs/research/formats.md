# Format notes

The implementation is based on observed `ULJS00061` and `ULJS00064` files and
the maintained parsers in the NGE2 translation project.

- ISO access is extent-based and directory records are read on demand.
- HGAR v1 stores offsets followed by member headers; v3 also has a global name
  hash table and aligned long-name records.
- HGAR's compressed member flag is bit 31 of the encoded identifier. The member
  body begins with a little-endian decompressed size followed by raw DEFLATE.
- HGPT supports 4-bit palette (`0x14`), 8-bit palette (`0x13`) and RGBA
  (`0x8800`) tiled pixels. Alpha values use the game's doubled 7-bit encoding.
- EVS has an offset table. Each record has a two-byte opcode and two-byte body
  size. Unknown records are retained as raw payload and do not stop parsing.

The preview is deliberately linear. It does not claim to emulate the game's
conditional control flow or full script virtual machine.
