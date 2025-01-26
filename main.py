#!/usr/bin/env python3
import os
import re
import sys
import shutil
import zipfile
from pathlib import Path
from typing import Tuple

try:
    from rich.console import Console
    from rich.progress import Progress, BarColumn, TimeRemainingColumn
    from rich.prompt import Prompt, Confirm
    from rich.table import Table
    from PIL import Image
    import moviepy.editor as mpy
except ImportError as e:
    print(
        f"Error: {e}\nPlease install requirements with: pip install -r requirements.txt"
    )
    sys.exit(1)

console = Console()
error_console = Console(stderr=True, style="bold red")


def clear_screen():
    """
    Clear the console screen.
    """
    console.print("\033c", end="")


def show_help():
    """
    Display a help message with available commands.
    """
    table = Table(title="Available Commands", show_header=True)
    table.add_column("Command", style="cyan")
    table.add_column("Description", style="magenta")

    commands = [
        ("help", "Show this help message"),
        ("getinfo", "Get video file information"),
        ("vid2jpg", "Convert video to JPG sequence"),
        ("zip2vid", "Convert bootanimation ZIP to video"),
        ("resize", "Resize video while preserving aspect ratio"),
        ("sort", "Organize JPG sequence into sections"),
        ("unsort", "Revert sorted sections back to main folder"),
        ("compress", "Compress folder using ZIP Store mode"),
        ("uncompress", "Extract ZIP archive"),
        ("exit", "Exit the application"),
    ]

    for cmd, desc in commands:
        table.add_row(cmd, desc)

    console.print(table)


def get_video_info(file_path: Path) -> dict:
    """
    Retrieve information about a video file, including
    duration, FPS, width, and height.

    :param file_path: Path object pointing to the video file.
    :return: Dictionary containing video metadata.
    """
    try:
        with mpy.VideoFileClip(str(file_path)) as clip:
            return {
                "path": clip.filename,
                "duration": clip.duration,
                "fps": clip.fps,
                "width": clip.w,
                "height": clip.h,
            }
    except Exception as e:
        raise RuntimeError(f"Failed to get video info: {e}")


def handle_getinfo():
    """
    Prompt for a video file path and print its information.
    """
    file_path = Prompt.ask("Enter video file path", console=console)
    if not Path(file_path).exists():
        error_console.print("File not found!")
        return

    try:
        info = get_video_info(Path(file_path))
        table = Table(title="Video Information", show_header=True)
        table.add_column("Property", style="cyan")
        table.add_column("Value", style="green")

        for key, value in info.items():
            table.add_row(key.upper(), str(value))

        console.print(table)
    except Exception as e:
        error_console.print(f"Error: {e}")


def handle_vid2jpg():
    """
    Prompt for a video file path and convert it to a sequence of JPG images.
    """
    file_path = Prompt.ask("Enter video file path", console=console)
    if not Path(file_path).exists():
        error_console.print("File not found!")
        return

    output_dir = Path(Path(file_path).stem)
    output_dir.mkdir(exist_ok=True)

    try:
        with mpy.VideoFileClip(str(file_path)) as clip:
            fps = clip.fps
            duration = clip.duration
            total_frames = int(fps * duration)
            digits = len(str(total_frames))

            with Progress(
                "[progress.description]{task.description}",
                BarColumn(),
                "[progress.percentage]{task.percentage:>3.0f}%",
                TimeRemainingColumn(),
                console=console,
            ) as progress:
                task = progress.add_task(
                    "[cyan]Converting frames...", total=total_frames
                )

                for i, frame in enumerate(clip.iter_frames()):
                    img = Image.fromarray(frame)
                    img.save(
                        output_dir / f"{i:0{digits}d}.jpg",
                        quality=100,
                        optimize=False,
                    )
                    progress.update(task, advance=1)

            with open(output_dir / "desc.txt", "w") as f:
                f.write(f"{clip.w} {clip.h} {int(fps)}")

            console.print(f"[green]Success![/] JPG sequence created in {output_dir}/")
    except Exception as e:
        error_console.print(f"Error: {e}")
        shutil.rmtree(output_dir, ignore_errors=True)


def handle_resize():
    """
    Prompt for a video file path, dimension type (width/height),
    and resize the video accordingly.
    """
    file_path = Prompt.ask("Enter video file path", console=console)
    if not Path(file_path).exists():
        error_console.print("File not found!")
        return

    dimension = Prompt.ask(
        "Resize by (width/height)", choices=["width", "height"], console=console
    )
    target = int(Prompt.ask(f"Enter target {dimension}", console=console))

    try:
        with mpy.VideoFileClip(str(file_path)) as clip:
            if dimension == "width":
                new_height = int(target * clip.h / clip.w)
                new_size = (target, new_height)
            else:
                new_width = int(target * clip.w / clip.h)
                new_size = (new_width, target)

            resized = clip.resize(new_size)
            output_path = (
                f"{Path(file_path).stem}_{new_size[0]}x{new_size[1]}"
                f"{Path(file_path).suffix}"
            )
            resized.write_videofile(output_path)

            console.print(f"[green]Success![/] Resized video saved as {output_path}")
    except Exception as e:
        error_console.print(f"Error: {e}")


def detect_prefix(folder: Path) -> Tuple[str, int]:
    """
    Detect the file prefix and digit count for a sequence of JPG files
    in a given folder.

    :param folder: Path to the folder containing JPG files.
    :return: A tuple of (prefix, digit_count).
    """
    files = list(folder.glob("*.jpg"))
    if not files:
        raise ValueError("No JPG files found in folder")

    sample = files[0].stem
    prefix = re.sub(r"\d+$", "", sample)
    digits = len(sample) - len(prefix)
    return prefix, digits


def handle_sort():
    """
    Prompt for a folder containing a JPG sequence. Sort images
    into sections, updating desc.txt with the new sections.
    """
    folder = Prompt.ask("Enter JPG sequence folder", console=console)
    folder = Path(folder)

    try:
        with open(folder / "desc.txt") as f:
            _, _, fps = map(int, f.readline().split())

        prefix, digits = detect_prefix(folder)
        sections = int(Prompt.ask("Number of sections", console=console))
        sections_info = []
        current_frame = 0

        for i in range(1, sections + 1):
            console.rule(f"Section {i}")

            end_time = Prompt.ask(
                "End time (seconds/(r for remaining part))", console=console
            )

            end_frame = (
                int(sorted(folder.glob("*.jpg"))[-1].stem[-digits:])
                if end_time == "r"
                else int(float(end_time) * fps)
            )

            section_type = Prompt.ask("Type (c/p)", choices=["c", "p"], console=console)
            looped = Confirm.ask("Loop section?", console=console)
            count = 0 if looped else int(Prompt.ask("Play count", console=console))

            section_folder = folder / f"S{i}"
            section_folder.mkdir(exist_ok=True)

            for frame in range(current_frame, end_frame + 1):
                src = folder / f"{prefix}{frame:0{digits}d}.jpg"
                dest = section_folder / src.name
                src.rename(dest)

            sections_info.append(f"{section_type} {count} 0 S{i}")
            current_frame = end_frame + 1

        with open(folder / "desc.txt", "a") as f:
            f.write("\n" + "\n".join(sections_info) + "\n")

        console.print("[green]Sorting completed successfully![/]")
    except Exception as e:
        error_console.print(f"Error: {e}")


def handle_unsort():
    """
    Prompt for a folder containing sorted JPG sections.
    Move files back to the main folder and remove section dirs.
    """
    folder = Prompt.ask("Enter sorted folder", console=console)
    folder = Path(folder)

    try:
        for subdir in folder.iterdir():
            if subdir.is_dir() and subdir.name.startswith("S"):
                for file in subdir.iterdir():
                    if file.suffix == ".jpg":
                        dest = folder / file.name
                        file.rename(dest)
                subdir.rmdir()

        console.print("[green]Unsort completed successfully![/]")
    except Exception as e:
        error_console.print(f"Error: {e}")


def handle_compress():
    """
    Prompt for a folder and compress it into a ZIP file
    using the ZIP_STORED compression mode.
    """
    folder = Prompt.ask("Enter folder to compress", console=console)
    folder = Path(folder)
    zip_name = f"{folder.name}.zip"

    try:
        with zipfile.ZipFile(zip_name, "w", compression=zipfile.ZIP_STORED) as zipf:
            for root, _, files in os.walk(folder):
                for file in files:
                    zipf.write(
                        os.path.join(root, file),
                        arcname=os.path.relpath(os.path.join(root, file), folder),
                    )

        console.print(f"[green]Compression completed![/]\nPath - {zip_name}")
    except Exception as e:
        error_console.print(f"Error: {e}")


def handle_uncompress():
    """
    Prompt for a ZIP file path and extract its contents.
    """
    zip_file = Prompt.ask("Enter ZIP file path", console=console)
    zip_folder = Path(Path(zip_file).stem)
    zip_folder.mkdir(exist_ok=True)

    try:
        with zipfile.ZipFile(zip_file, "r") as zipf:
            zipf.extractall(zip_folder)

        console.print("[green]Extraction completed successfully![/]")
    except Exception as e:
        error_console.print(f"Error: {e}")


def handle_zip2vid():
    """
    Convert a bootanimation ZIP to a video by extracting
    JPG frames and creating a final MP4 file.
    """
    zip_file = Prompt.ask("Enter bootanimation ZIP path", console=console)
    if not Path(zip_file).exists():
        error_console.print("ZIP file not found!")
        return

    output_folder = Path(zip_file).stem
    try:
        with zipfile.ZipFile(zip_file, "r") as zipf:
            zipf.extractall(output_folder)
        console.print(f"[green]Extracted to {output_folder}/[/]")
    except Exception as e:
        error_console.print(f"Error extracting ZIP: {e}")
        return

    folder = Path(output_folder)

    try:
        section_dirs = [
            d for d in folder.iterdir() if d.is_dir() and d.name.startswith("S")
        ]
        if section_dirs:
            console.print("[yellow]Found sorted sections, unsorting...[/]")
            for section_dir in section_dirs:
                for img_file in section_dir.glob("*.jpg"):
                    dest = folder / img_file.name
                    if dest.exists():
                        error_console.print(
                            f"Conflict: {img_file.name} already exists in main folder!"
                        )
                        return
                    img_file.rename(dest)
                section_dir.rmdir()
    except Exception as e:
        error_console.print(f"Error during unsorting: {e}")
        return

    try:
        with open(folder / "desc.txt", "r") as f:
            _, _, fps = map(int, f.readline().strip().split())
    except Exception as e:
        error_console.print(f"Error reading desc.txt: {e}")
        return

    try:
        jpg_files = list(folder.glob("*.jpg"))
        if not jpg_files:
            error_console.print("No JPG files found in folder!")
            return

        jpg_files.sort(key=lambda x: int(re.search(r"\d+", x.stem).group()))
    except Exception as e:
        error_console.print(f"Error sorting JPG files: {e}")
        return

    output_path = f"{folder.name}_converted.mp4"
    try:
        with console.status("[bold green]Preparing frames...[/]", spinner="dots"):
            clip = mpy.ImageSequenceClip([str(img) for img in jpg_files], fps=fps)

        with Progress(
            "[progress.description]{task.description}",
            BarColumn(),
            "[progress.percentage]{task.percentage:>3.0f}%",
            TimeRemainingColumn(),
            console=console,
        ) as progress:
            rendering_task = progress.add_task(
                "[cyan]Rendering video...", total=len(jpg_files)
            )

            def make_frame(gf, t):
                progress.update(rendering_task, advance=1)
                return gf(t)

            clip = clip.fl(make_frame, apply_to=["video"])
            clip.write_videofile(
                output_path,
                codec="libx264",
                fps=fps,
                verbose=False,
                logger=None,
                threads=4,
            )

        console.print(
            f"[green]Video created successfully![/]\nPath: [cyan]{output_path}[/]"
        )
    except Exception as e:
        error_console.print(f"Error creating video: {e}")
        return


def main():
    """
    Main entry point for the application. Clears the screen, prints
    a welcome message, and handles user commands in a loop.
    """
    clear_screen()
    console.print("[bold magenta]Boot Animation Creator[/]\n", justify="center")
    console.print("Type 'help' for available commands\n")

    handlers = {
        "help": show_help,
        "getinfo": handle_getinfo,
        "vid2jpg": handle_vid2jpg,
        "resize": handle_resize,
        "sort": handle_sort,
        "unsort": handle_unsort,
        "compress": handle_compress,
        "uncompress": handle_uncompress,
        "zip2vid": handle_zip2vid,
    }

    while True:
        try:
            cmd = Prompt.ask(">>", console=console).strip().lower()
            if cmd == "exit":
                console.print("[bold]Goodbye![/]")
                break
            elif cmd in handlers:
                handlers[cmd]()
            else:
                error_console.print(
                    "Invalid command! Type 'help' for available commands"
                )
        except KeyboardInterrupt:
            console.print("\n[bold]Goodbye![/]")
            break
        except Exception as e:
            error_console.print(f"Unexpected error: {e}")


if __name__ == "__main__":
    main()
