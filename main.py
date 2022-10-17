#!/usr/bin/env python3

# Import Dependencies
import colorama
import cv2
from os import system as run, name as osName, mkdir, path as ospath, chdir as cd, remove as rm, listdir as ls
from random import randint
from PIL import Image
from shutil import get_terminal_size, rmtree

# Define Parameters
commands = {
    # "command" :   "description"

    "help"      :   "Displays this Message",
    "clear"     :   "Clears the Shell",
    "convert"   :   "Converts a video to a PNG sequence",
    "build"     :   "Builds the Bootanimation",
    "preview"   :   "Renders a png sequence into a video",
    "exit"      :   "Exit the Program",
    "bot"       :   "Run the MercuryX Bot on Telegram",
    "buildx"    :   "Automatically Builds the Bootanimation"
    }

cmd = list(commands) # Initialise Command List
val = list(commands.values()) # Description

spaceline = " " * get_terminal_size().columns # A string full of spaces
clearConsole = lambda: run('cls' if osName in ('nt', 'dos') else 'clear') # Native Clear Function

# Art
def figlet():
    print(colorama.Fore.BLUE +    "  ____              _      _          _           _       ")
    print(colorama.Fore.GREEN +   " | __ )  ___   ___ | |_   / \   _ __ (_)_ __ ___ (_)_  __ ")
    print(colorama.Fore.MAGENTA + " |  _ \ / _ \ / _ \| __| / _ \ | '_ \| | '_ ` _ \| \ \/ / ")
    print(colorama.Fore.YELLOW +  " | |_) | (_) | (_) | |_ / ___ \| | | | | | | | | | |>  <  ")
    print(colorama.Fore.RED +     " |____/ \___/ \___/ \__/_/   \_\_| |_|_|_| |_| |_|_/_/\_\ ")
    print(colorama.Fore.RESET +   " ")

# Convert Video to PNG Sequence
def convert():
    # Collect Paths
    pathIn = input(" Enter the path of the video: ")
    pathOut = ospath.join(ospath.dirname(pathIn), ospath.splitext(pathIn)[0])

    # Initiate Outcomes
    if ospath.exists(pathOut) is True:
        pathOut = pathOut + str(randint(0, 100000))
        if ospath.exists(pathOut) is True:
            rmtree(pathOut) 
        mkdir(pathOut)
        cd(pathOut)
    elif ospath.exists(pathOut) is False:
        mkdir(pathOut)
        cd(pathOut)
    else:
        exit()

    # Convert Video to JPG Sequence
    vidcap = cv2.VideoCapture(pathIn)
    frame_count = int(vidcap.get(cv2.CAP_PROP_FRAME_COUNT))
    success,image = vidcap.read()
    count = 0
    success = True
    while success:
        success,image = vidcap.read()
        zcount = str(count).zfill(len(str(frame_count)))
        if success is False:
          break
        cv2.imwrite(ospath.join(pathOut, f"part{zcount}.jpg"), image)
        count += 1
        print(f" Extracted: {count}/{frame_count} Frames", end='\r')
    print(spaceline, end="\r")
    vidcap.release()

    # Convert JPG Sequence to PNG Sequence
    count = 0
    files_in_directory = ls(pathOut)
    filtered_files = [file for file in files_in_directory if file.endswith(".jpg")]
    for file in filtered_files:
        path_to_file = ospath.join(pathOut, file)
        Image.open(path_to_file).save(ospath.join(pathOut, ospath.splitext(path_to_file)[0]) + '.png', format="png", compress_level=9)
        rm(path_to_file)
        count += 1
        print(f" Converted: {count}/{frame_count} Files", end='\r')
    print (spaceline, end="\r")

    # Print and Exit
    print(colorama.Fore.CYAN + colorama.Style.DIM + f" PNG Sequence created in {pathOut}" + colorama.Style.RESET_ALL, end="\r")
    print("\n")

# Initiate Main Shell
def shell():
    cmdcount = 0 # Initialise Command Counter
    while True: # Loop Shell Input
        shellIn = input(" BootAnimix > ").lower().split() # Initialise Shell Input
        
        # Check for empty input
        if shellIn == []:
            continue
        else:
            shellIn = shellIn[0]

        # Check for valid commands
        if shellIn in cmd:
            pass
        else:
            print(colorama.Fore.RED + " " + shellIn + ": Command not found." + colorama.Fore.RESET + "\n")

        # Check for help command
        if shellIn == cmd[0]:
            if cmdcount == 0:
                clearConsole()
                figlet()
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Avaliable Executables:' + colorama.Style.RESET_ALL + "\n")
            for i in range(len(commands)):
                print (f"  {i+1}.  {cmd[i]} - {val[i]}")
            print("") # Blank padding

        # Check for clear command
        elif shellIn == cmd[1]:
            clearConsole()
            figlet()

        # Check for exit command
        elif shellIn == cmd[5]:
            clearConsole()
            break

        # Check for convert command
        elif shellIn == cmd[2]:
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Convert Video to PNG Sequence' + colorama.Style.RESET_ALL + "\n")
            convert()

        cmdcount += 1 # Increment Command Counter

# Initiate The process
colorama.init() # Initialise Colorama
clearConsole() # Clear Console
figlet() # Print Figlet
print(colorama.Fore.BLUE + colorama.Style.BRIGHT + " Welcome to BootAnimix Shell, Type 'help' for more information" + "\n" + colorama.Style.RESET_ALL) # Print Welcome Message
shell() # Run Shell
