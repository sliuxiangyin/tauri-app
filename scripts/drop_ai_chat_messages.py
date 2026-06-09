#!/usr/bin/env python3
"""清除旧表数据和迁移记录，用于迁移文件重建后重新初始化数据库"""
import sqlite3
import sys

DB_PATH = r"C:\Users\woddp\AppData\Roaming\com.woddp.tauri-app\app.db"

# 需要清除的表列表（按依赖顺序：先删子表，再删主表）
TABLES_TO_DROP = [
    "conversations",   # 内容块表
    "plans",           # 计划表
    "messages",        # 消息索引表
    "ai_chat_messages", # 旧版扁平消息表（如存在）
]


def main():
    # 检查数据库文件是否存在
    import os
    if not os.path.exists(DB_PATH):
        print(f"数据库文件不存在: {DB_PATH}")
        print("无需清理，直接启动应用即可。")
        return

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    # 打印当前迁移状态
    print("=" * 50)
    print("当前迁移记录:")
    print("=" * 50)
    try:
        cur.execute("SELECT version, applied_at FROM seaql_migrations ORDER BY applied_at")
        migrations = cur.fetchall()
        if migrations:
            for version, applied_at in migrations:
                print(f"  ✓ {version}")
        else:
            print("  (无迁移记录)")
    except sqlite3.OperationalError:
        print("  (seaql_migrations 表不存在)")

    # 打印所有表
    print("\n" + "=" * 50)
    print("当前数据库表:")
    print("=" * 50)
    cur.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
    tables = cur.fetchall()
    if tables:
        for (name,) in tables:
            cur.execute(f"SELECT COUNT(*) FROM [{name}]")
            count = cur.fetchone()[0]
            print(f"  - {name} ({count} 行)")
    else:
        print("  (无表)")

    # 清除所有迁移记录
    print("\n" + "=" * 50)
    print("清除迁移记录:")
    print("=" * 50)
    try:
        cur.execute("DELETE FROM seaql_migrations")
        print(f"  已清除所有迁移记录")
    except sqlite3.OperationalError as e:
        print(f"  (seaql_migrations 表不存在或无法删除: {e})")

    # 删除指定的表
    print("\n" + "=" * 50)
    print("删除表:")
    print("=" * 50)
    for table_name in TABLES_TO_DROP:
        # 检查表是否存在
        cur.execute(
            "SELECT name FROM seaql_migrations WHERE type='table' AND name=?",
            (table_name,),
        )
        if cur.fetchone() is None:
            print(f"  [跳过] {table_name} 不存在")
            continue

        # 删除表
        cur.execute(f"DROP TABLE [{table_name}]")
        print(f"  [删除] {table_name}")

    conn.commit()
    conn.close()

    print("\n" + "=" * 50)
    print("完成！重新启动应用将自动重建表结构。")
    print("=" * 50)


if __name__ == "__main__":
    main()

