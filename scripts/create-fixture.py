#!/usr/bin/env python3
"""Create an isolated benchmark fixture; never accepts an existing directory."""
import argparse
import pathlib
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('destination', type=pathlib.Path)
parser.add_argument('--files', type=int, default=10000)
parser.add_argument('--repos', type=int, default=10)
parser.add_argument('--changed', type=int, default=100)
args = parser.parse_args()
if args.destination.exists():
    parser.error('destination must not already exist')
if args.files < 1 or not 1 <= args.repos <= args.files:
    parser.error('require files >= repos >= 1')
args.destination.mkdir(parents=True)
for repo_index in range(args.repos):
    repo = args.destination / f'repos/repo-{repo_index:03d}'
    repo.mkdir(parents=True)
    subprocess.run(['git', 'init', '-q', '-b', 'main', str(repo)], check=True)
    count = args.files // args.repos + (repo_index < args.files % args.repos)
    for file_index in range(count):
        folder = repo / f'src/group-{file_index // 100:04d}'
        folder.mkdir(exist_ok=True, parents=True)
        (folder / f'file-{file_index:06d}.ts').write_text(f'export const value = {file_index};\n')
    subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
    subprocess.run(['git', '-C', str(repo), '-c', 'user.name=GitVista Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture'], check=True)
    changed = min(count, args.changed // args.repos + (repo_index < args.changed % args.repos))
    for file_index in range(changed):
        (repo / f'src/group-{file_index // 100:04d}/file-{file_index:06d}.ts').write_text(f'export const value = {file_index + 1};\n')
print(f'{args.destination}: {args.files} files in {args.repos} repositories')
