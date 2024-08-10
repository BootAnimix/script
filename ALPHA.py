# Import Dependencies

import cv2
from tqdm import tqdm
import os
import shutil
import re
import zipfile


# Define Core Functions


def init() -> None:
    print(
        "\nBootAnimix 2.0.0 Command List -\n\n"
        "  convert  :  Convert a video to a folder of jpgs\n"
        "  revert   :  Convert a sequence of jpgs to a video\n"
        "  build    :  Build a bootanimation.zip file\n"
        "  help     :  Display this command list\n"
        "  exit     :  Exit the program\n"
    )


def video_to_jpgs(
    video_path: str,
    output_folder: str,
    resolution: tuple[int, int] = None,
    fps: int = None,
    verbose: bool = True,
) -> None:
    # Check if the output folder already exists
    if os.path.exists(output_folder):
        match input(
            "Output folder already exists. Do you want to overwrite it? (y/n): "
        ).lower():
            case "y":
                shutil.rmtree(output_folder)
            case "n":
                return

    # Create the output folder and the raw subfolder
    os.makedirs(output_folder)
    os.makedirs(os.path.join(output_folder, "raw"))

    # Open the video file and get its properties
    vidcap = cv2.VideoCapture(video_path)
    frame_count = int(vidcap.get(cv2.CAP_PROP_FRAME_COUNT))
    fps = (
        int(vidcap.get(cv2.CAP_PROP_FPS))
        if fps is None or int(vidcap.get(cv2.CAP_PROP_FPS)) % fps != 0
        else fps
    )
    skip = int(vidcap.get(cv2.CAP_PROP_FPS) / fps)

    if verbose:
        print("\nExtracting frames...")
        pbar = tqdm(total=frame_count, desc="Progress", unit="frames")

    zfill = len(str(frame_count))
    count = 0

    # Read each frame and save it as a jpg
    while True:
        if count % skip != 0:
            count += 1
            continue

        success, image = vidcap.read()
        zcount = str(count // skip).zfill(zfill)

        if not success:
            break
        if resolution:
            image = cv2.resize(image, resolution)

        cv2.imwrite(os.path.join(output_folder, "raw", f"{zcount}.jpg"), image)

        if verbose:
            pbar.update(1)
        count += 1

    if verbose:
        pbar.close()
    vidcap.release()

    height, width, _ = cv2.imread(
        os.path.join(output_folder, "raw", "0".zfill(zfill) + ".jpg")
    ).shape

    with open(os.path.join(output_folder, "desc.txt"), "w") as f:
        f.write(f"{width} {height} {fps}")


def jpgs_to_video(
    jpg_folder: str,
    output_path: str,
    width: int,
    height: int,
    fps: int,
    verbose: bool = True,
) -> None:
    # Check if the output video already exists
    if os.path.exists(output_path):
        match input(
            "Output video already exists. Do you want to overwrite it? (y/n): "
        ).lower():
            case "y":
                os.remove(output_path)
            case "n":
                return

    # Get all frames from the subfolders and sort them by their number
    images = sorted(
        [
            os.path.join(jpg_folder, sub_folder, img)
            for sub_folder in os.listdir(jpg_folder)
            if os.path.isdir(os.path.join(jpg_folder, sub_folder))
            for img in os.listdir(os.path.join(jpg_folder, sub_folder))
            if img.endswith(".jpg")
        ],
        key=lambda x: int(re.search(r"(\d+)", os.path.basename(x)).group(0)),
    )

    # Create the video writer object
    video = cv2.VideoWriter(
        output_path, cv2.VideoWriter_fourcc(*"mp4v"), fps, (width, height)
    )

    if verbose:
        print("Creating video...")
        pbar = tqdm(total=len(images), desc="Progress", unit="frames")

    # Write each frame to the video
    for image in images:
        video.write(cv2.imread(image))

        if verbose:
            pbar.update(1)

    if verbose:
        pbar.close()
    cv2.destroyAllWindows()
    video.release()


def convert(verbose: bool) -> None:
    print("\nConvert a video to a folder of jpgs\n")

    pathIn = input("Enter the path of the video: ")

    n_res = input(
        "Enter the number of resolutions to extract (Leave empty for original resolution only): "
    )
    resolutions = (
        [
            tuple(
                map(
                    int,
                    input(
                        f"Enter the resolution of Bootanimation {i + 1} (WIDTH HEIGHT): "
                    ).split(" "),
                )
            )
            for i in range(int(n_res))
        ]
        if n_res
        else [None]
    )

    for i, resolution in enumerate(resolutions):
        pathOut = os.path.join(
            os.path.dirname(pathIn), os.path.splitext(pathIn)[0] + f"_{i + 1}"
        )
        video_to_jpgs(pathIn, pathOut, resolution, verbose=verbose)
        print(f"Resolution {i + 1} extracted successfully.")

    print("\nVideo converted successfully.\n")


def revert(verbose: bool) -> None:
    print("\nConvert a sequence of jpgs to a video\n")

    pathIn = input("Enter the path of the jpg folder: ")
    pathOut = os.path.join(os.path.dirname(pathIn), os.path.basename(pathIn) + ".mp4")

    try:
        with open(os.path.join(pathIn, "desc.txt"), "r") as f:
            order = f.read().splitlines()[0]
            width, height, fps = map(int, order.split(" "))
    except FileNotFoundError:
        print("\ndesc.txt not found in the folder.")
        return
    except Exception as e:
        print(f"\nError: {e}")
        return

    jpgs_to_video(pathIn, pathOut, width, height, fps, verbose=verbose)
    print("\nVideo created successfully.\n")


def build() -> None:
    print("\nBuild a bootanimation.zip file\n")

    pathIn = input("Enter the path of the sequence folder: ")
    pathOut = os.path.join(os.path.dirname(pathIn), os.path.basename(pathIn) + ".zip")

    try:
        zipfile.ZipFile(pathOut, "w")
    except Exception as e:
        print(f"\nError: {e}")
        return
    
    with zipfile.ZipFile(pathOut, "w") as zipf:
        for folder in os.listdir(pathIn):
            if os.path.isdir(os.path.join(pathIn, folder)):
                for file in os.listdir(os.path.join(pathIn, folder)):
                    zipf.write(os.path.join(pathIn, folder, file), os.path.join(folder, file))
            else:
                zipf.write(os.path.join(pathIn, folder), folder)

    print("\nBootanimation.zip created successfully.\n")


# Define Main Function


def main() -> None:
    match input("Do you want to enable GUI mode? (y/n) : "):
        case "y":
            import tkinter as tk
        case "yes":
            import tkinter as tk
        case "no":
            tk = None
        case "n":
            tk = None

    match input("Do you want to enable verbose mode? (y/n) : "):
        case "y":
            verbose = True
        case "yes":
            verbose = True
        case "no":
            verbose = False
        case "n":
            verbose = False

    init()

    while True:
        match input(">> ").lower().strip():
            case "convert":
                convert(verbose)
            case "revert":
                revert(verbose)
            case "build":
                build()
            case "help":
                init()
            case "exit":
                break
            case "":
                continue
            case _:
                print("Invalid command. Please try again.")
                continue


# Execute the Script

if __name__ == "__main__":
    main()
