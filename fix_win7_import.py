#!/usr/bin/env python3
"""
Windows 7 兼容性 - PE 导入表修复工具
====================================

问题: Rust 编译的程序直接导入 GetSystemTimePreciseAsFileTime，
      该 API 仅在 Windows 8+ 的 kernel32.dll 中存在。
      PE 加载器在解析导入表时发现缺失→直接报错终止。

修复: 将 ILT (Import Lookup Table) 中指向
      GetSystemTimePreciseAsFileTime 的条目重定向到
      GetSystemTimeAsFileTime 的 hint/name 条目。
      PE 加载器将按修改后的名字去 kernel32.dll 中查找。
      Rust std 内部有运行时回退逻辑，行为完全正确。

用法: python fix_win7_import.py <exe_path>
依赖: pip install pefile
"""

import sys
import struct
from pathlib import Path

try:
    import pefile
except ImportError:
    print("[ERROR] pip install pefile")
    sys.exit(1)


def rva_to_file_offset(pe, rva):
    """将 RVA 转换为文件偏移量"""
    for section in pe.sections:
        if section.VirtualAddress <= rva < section.VirtualAddress + section.Misc_VirtualSize:
            return rva - section.VirtualAddress + section.PointerToRawData
    return None


def fix_import_table(exe_path: str) -> bool:
    target_name = b"GetSystemTimePreciseAsFileTime"
    replacement_name = b"GetSystemTimeAsFileTime"

    if not Path(exe_path).exists():
        print(f"[ERROR] 文件不存在: {exe_path}")
        return False

    try:
        pe = pefile.PE(exe_path)
    except Exception as e:
        print(f"[ERROR] 解析 PE 失败: {e}")
        return False

    # 找到 kernel32.dll 的导入条目
    target_ilt_rva = None
    replacement_hint_name_rva = None  # hint/name struct of replacement

    for entry in pe.DIRECTORY_ENTRY_IMPORT:
        dll = entry.dll.decode('utf-8', errors='ignore').lower()
        if 'kernel32' not in dll:
            continue
        
        # First pass: locate both entries and their hint/name RVAs
        prec_ilt = None  # ILT thunk RVA for precise version
        asfile_hint_rva = None  # hint/name RVA for AsFileTime version

        for imp in entry.imports:
            if imp.name == target_name:
                # imp.thunk_rva is the ILT entry RVA
                # This ILT entry currently points to the hint/name of the precise version
                prec_ilt = imp.thunk_rva
                target_ilt_rva = imp.thunk_rva
                print(f"[INFO] 找到 {imp.name.decode()} (ILT RVA: 0x{prec_ilt:08X})")
            elif imp.name == replacement_name:
                # We need the hint/name struct RVA of the as-file-time version
                asfile_hint_rva = imp.hint_name_table_rva
                replacement_hint_name_rva = imp.hint_name_table_rva
                print(f"[INFO] 找到 {imp.name.decode()} (hint/name RVA: 0x{asfile_hint_rva:08X})")

        if target_ilt_rva and replacement_hint_name_rva:
            # Get file offset of the ILT entry
            ilt_file_offset = rva_to_file_offset(pe, target_ilt_rva)
            if ilt_file_offset is None:
                print(f"[ERROR] 无法定位 ILT RVA 0x{target_ilt_rva:08X} 的文件偏移")
                return False

            # Read the ILT entry (8 bytes for x64)
            is_64bit = pe.OPTIONAL_HEADER.Magic == 0x20b
            entry_size = 8 if is_64bit else 4
            
            with open(exe_path, 'r+b') as f:
                f.seek(ilt_file_offset)
                ilt_data = f.read(entry_size)
                old_thunk = struct.unpack('<Q' if is_64bit else '<I', ilt_data)[0]
                
                print(f"[INFO] 当前 ILT 值: 0x{old_thunk:016X}" if is_64bit else f"[INFO] 当前 ILT 值: 0x{old_thunk:08X}")
                print(f"[INFO] 新 ILT 值 (指向 {replacement_name.decode()}): 0x{replacement_hint_name_rva:08X}")
                
                # Write new ILT value (hint/name RVA, with high bit clear = import by name)
                new_thunk = replacement_hint_name_rva
                f.seek(ilt_file_offset)
                f.write(struct.pack('<Q' if is_64bit else '<I', new_thunk))
                
            print(f"[OK]   ILT 条目已重定向: {target_name.decode()} → {replacement_name.decode()}")
            
            # Verify
            pe2 = pefile.PE(exe_path)
            for entry2 in pe2.DIRECTORY_ENTRY_IMPORT:
                d = entry2.dll.decode('utf-8', errors='ignore').lower()
                if 'kernel32' in d:
                    for imp2 in entry2.imports:
                        if imp2.thunk_rva == target_ilt_rva:
                            print(f"[OK]   验证: 现在解析为 {imp2.name.decode() if imp2.name else f'Ordinal({imp2.ordinal})'}")
                            break
                    break
            
            print(f"[OK]   已保存: {exe_path}")
            return True

    if target_ilt_rva is None:
        print("[INFO] 未找到 GetSystemTimePreciseAsFileTime 导入")
        print("[INFO] PE 文件可能已经兼容 Win7")
        return True

    print("[ERROR] 未找到 GetSystemTimeAsFileTime 导入（无法重定向）")
    return False


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        print("用法: python fix_win7_import.py <exe_path>")
        sys.exit(1)
    
    success = fix_import_table(sys.argv[1])
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()