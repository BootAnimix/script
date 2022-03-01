#!/usr/bin/env python3

# Import Dependencies
import colorama
import cv2
from os import system as run, name as osName, mkdir, path as ospath, chdir as cd, remove as rm, listdir as ls
from random import randint
from PIL import Image
from shutil import get_terminal_size

# Define Parameters
commands = {
    # "command" :   "description"

    "help"      :   "Displays this Message",
    "clear"     :   "Clears the Shell",
    "convert"   :   "Converts a video to a PNG sequence",
    "compress"  :   "Compresses a video/PNG sequence",
    "build"     :   "Build the Bootanimation",
    "preview"   :   "Renders a png sequence into a video",
    "exit"      :   "Exit the Program",
    "bot"       :   "Run the MercuryX Bot on Telegram"
    }

cmd = list(commands)
val = list(commands.values())
clearlast = " " * get_terminal_size().columns

class OverwriteLast:
    def __init__(self):
        self.last = 0
    def print(self,s):
        if self.last:
            print(' '*self.last, end='\r')
        self.last = len(s)
        print(s, end='\r')

over = OverwriteLast()

clearConsole = lambda: run('cls' if osName in ('nt', 'dos') else 'clear')
def figlet():
    print(colorama.Fore.BLUE +    "  ____              _      _          _           _       ")
    print(colorama.Fore.GREEN +   " | __ )  ___   ___ | |_   / \   _ __ (_)_ __ ___ (_)_  __ ")
    print(colorama.Fore.MAGENTA + " |  _ \ / _ \ / _ \| __| / _ \ | '_ \| | '_ ` _ \| \ \/ / ")
    print(colorama.Fore.YELLOW +  " | |_) | (_) | (_) | |_ / ___ \| | | | | | | | | | |>  <  ")
    print(colorama.Fore.RED +     " |____/ \___/ \___/ \__/_/   \_\_| |_|_|_| |_| |_|_/_/\_\ ")
    print(colorama.Fore.RESET +   " ")

def convert():
    pathIn = input(" Enter the path of the video: ")
    pathOut = ospath.join(ospath.dirname(pathIn), ospath.splitext(pathIn)[0])

    if ospath.exists(pathOut) is True:
        pathOut = pathOut + str(randint(0, 100000))
        mkdir(pathOut)
        cd(pathOut)
    elif ospath.exists(pathOut) is False:
        mkdir(pathOut)
        cd(pathOut)
    else:
        exit()

    vidcap = cv2.VideoCapture(pathIn)
    frame_count = int(vidcap.get(cv2.CAP_PROP_FRAME_COUNT))
    success,image = vidcap.read()
    count = 0
    success = True
    print(" ")
    while success:
        success,image = vidcap.read()
        if success is False:
            break
        cv2.imwrite(pathOut + "\\part%d.jpg" % count, image)
        count += 1
        print(f" Extracted: {count}/{frame_count} Frames", end='\r')
    print(clearlast, end="\r")
    vidcap.release()

    count = 0
    files_in_directory = ls(pathOut)
    filtered_files = [file for file in files_in_directory if file.endswith(".jpg")]
    for file in filtered_files:
        path_to_file = ospath.join(pathOut, file)
        Image.open(path_to_file).save(ospath.join(pathOut, ospath.splitext(path_to_file)[0]) + '.png', format="png")
        rm(path_to_file)
        count += 1
        print(f" Converted: {count}/{frame_count} Files", end='\r')
    print (clearlast, end="\r")

    print(colorama.Fore.CYAN + colorama.Style.DIM + f" PNG Sequence created in {pathOut}" + colorama.Style.RESET_ALL, end="\r")
    print(" ")

def shell():
    cmdcount = 0
    while True:
        shellIn = input(" BootAnimix > ").lower().split()
        
        if shellIn == []:
            continue
        else:
            shellIn = shellIn[0]

        if shellIn in cmd:
            pass
        else:
            print(colorama.Fore.RED + shellIn + ": Command not found." + colorama.Fore.RESET + "\n")

        if shellIn == cmd[0]:
            if cmdcount == 0:
                clearConsole()
                figlet()
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Avaliable Executables:' + colorama.Style.RESET_ALL + "\n")
            for i in range(len(commands)):
                print (f"  {i+1}.  {cmd[i]} - {val[i]}")
            print("") # Blank padding

        elif shellIn == cmd[1]:
            clearConsole()
            figlet()

        elif shellIn == cmd[6]:
            clearConsole()
            break

        elif shellIn == cmd[2]:
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Convert Video to PNG Sequence' + colorama.Style.RESET_ALL + "\n")
            convert()

        cmdcount += 1

    return shellIn

# Initiate The process
colorama.init()
clearConsole()
figlet()
print(colorama.Fore.BLUE + colorama.Style.BRIGHT + " Welcome to BootAnimix Shell, Type 'help' for more information" + "\n" + colorama.Style.RESET_ALL)
shell()
