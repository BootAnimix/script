# Import Dependencies
import colorama
import cv2
from os import system as run, name as osName, mkdir, path as ospath, chdir as cd
from random import randint
from shutil import get_terminal_size

# Define Parameters
commands = {
    "help"          :   "Displays this Message",
    "clear"         :   "Clears the Shell",
    "convert"       :   "Converts a video to a JPG sequence",
    "convert-nogui" :   "Converts a video to a JPG sequence without GUI",
    "exit"          :   "Exit the Program",
    }

cmd = list(commands)
val = list(commands.values())
clearConsole = lambda: run('cls' if osName in ('nt', 'dos') else 'clear')

def spaceline():
    return " " * get_terminal_size().columns

# Art
def figlet():
    print(colorama.Fore.BLUE +    "  ____              _      _          _           _       ")
    print(colorama.Fore.GREEN +   " | __ )  ___   ___ | |_   / \   _ __ (_)_ __ ___ (_)_  __ ")
    print(colorama.Fore.MAGENTA + " |  _ \ / _ \ / _ \| __| / _ \ | '_ \| | '_ ` _ \| \ \/ / ")
    print(colorama.Fore.YELLOW +  " | |_) | (_) | (_) | |_ / ___ \| | | | | | | | | | |>  <  ")
    print(colorama.Fore.RED +     " |____/ \___/ \___/ \__/_/   \_\_| |_|_|_| |_| |_|_/_/\_\ ")
    print(colorama.Fore.RESET +   " ")

# Convert Video to PNG Sequence
def convert(nogui):
    if nogui:
        pathIn = input(" Enter the path of the video: ")
    else:
        import tkinter.filedialog as filedialog

        filetypes = [("Video Files", "*.mp4;*.avi;*.mkv;*.mov;*.wmv")]
        pathIn = filedialog.askopenfilename(filetypes=filetypes)
        if not pathIn:
            return

    pathOut = ospath.join(ospath.dirname(pathIn), ospath.splitext(pathIn)[0])

    # Initiate Outcomes
    if ospath.exists(pathOut):
        pathOut = pathOut + "-" + str(randint(0, 1000000))
        mkdir(pathOut)
        cd(pathOut)
    else:
        mkdir(pathOut)
        cd(pathOut)

    # Convert Video to JPG Sequence
    vidcap = cv2.VideoCapture(pathIn)
    frame_count = int(vidcap.get(cv2.CAP_PROP_FRAME_COUNT))
    success,image = vidcap.read()
    count = 0
    success = True

    # Get Resolution
    height, width, layers = image.shape

    if nogui:
        print(" ")

    while success:
        success, image = vidcap.read()
        zcount = str(count).zfill(len(str(frame_count)))

        if not success:
            break

        # Extract frames for 1440p resolution
        pathOut1440p = ospath.join(ospath.dirname(pathIn), ospath.splitext(pathIn)[0] + "_1440p")
        if ospath.exists(pathOut1440p):
            pass
        else:
            mkdir(pathOut1440p)
            cd(pathOut1440p)
        image1440p = cv2.resize(image, (1440, 2560))
        cv2.imwrite(ospath.join(pathOut1440p, f"part{zcount}.jpg"), image1440p)

        # Extract frames for 1080p resolution
        pathOut1080p = ospath.join(ospath.dirname(pathIn), ospath.splitext(pathIn)[0] + "_1080p")
        if ospath.exists(pathOut1080p):
            pass
        else:
            mkdir(pathOut1080p)
            cd(pathOut1080p)
        image1080p = cv2.resize(image, (1080, 1920))
        cv2.imwrite(ospath.join(pathOut1080p, f"part{zcount}.jpg"), image1080p)

        # Extract frames for 720p resolution
        pathOut720p = ospath.join(ospath.dirname(pathIn), ospath.splitext(pathIn)[0] + "_720p")
        if ospath.exists(pathOut720p):
            pass
        else:
            mkdir(pathOut720p)
            cd(pathOut720p)
        if count % 2 == 0:
            image720p = cv2.resize(image, (720, 1280))
            cv2.imwrite(ospath.join(pathOut720p, f"part{str(int(count/2)).zfill(len(str(frame_count)))}.jpg"), image720p)

        # Extract frames for Original resolution
        pathOut = ospath.join(ospath.dirname(pathIn), ospath.splitext(pathIn)[0])
        if ospath.exists(pathOut):
            pass
        else:
            mkdir(pathOut)
            cd(pathOut)
        cv2.imwrite(ospath.join(pathOut, f"part{zcount}.jpg"), image)

        count += 1
        print(f" Extracted: {count}/{frame_count} Frames", end='\r')

    # Print and Exit
    print(colorama.Fore.CYAN + colorama.Style.DIM + f" JPG Sequences created in {pathOut1440p}, {pathOut1080p}, and {pathOut720p}" + colorama.Style.RESET_ALL, end="\r")
    print("\n")

    print(spaceline(), end="\r")
    vidcap.release()

    # Print and Exit
    print(colorama.Fore.CYAN + colorama.Style.DIM + f" JPG Sequence created in {pathOut}" + colorama.Style.RESET_ALL, end="\r")
    print("\n")

# Initiate Main Shell
def shell():
    cmdcount = 0 # Initialise Command Counter

    while True: # Loop Shell Input

        shellIn = input(" BootAnimix > ").lower().strip()
        
        # Check for empty input
        if shellIn == "":
            continue

        # Check for valid commands
        if shellIn not in cmd:
            print(colorama.Fore.RED + " " + shellIn + ": Command not found." + colorama.Fore.RESET + "\n")

        # Check for help command
        if shellIn == cmd[0]:
            if cmdcount == 0:
                clearConsole()
                figlet()

            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Avaliable Executables:' + colorama.Style.RESET_ALL + "\n")

            for i in range(len(commands)):
                print (f"  {i+1}.  {cmd[i]} - {val[i]}")

            print("")

        # Check for clear command
        elif shellIn == cmd[1]:
            clearConsole()
            figlet()

        # Check for exit command
        elif shellIn == cmd[4]:
            clearConsole()
            break

        # Check for convert command
        elif shellIn == cmd[2]:
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Convert Video to JPG Sequence' + colorama.Style.RESET_ALL + "\n")
            convert(nogui=False)
        
        # Check for convert-nogui command
        elif shellIn == cmd[3]:
            print(colorama.Fore.MAGENTA + colorama.Style.BRIGHT + ' Convert Video to JPG Sequence' + colorama.Style.RESET_ALL + "\n")
            convert(nogui=True)

        cmdcount += 1 # Increment Command Counter

# Initiate The process

colorama.init()
clearConsole()
figlet()
print(colorama.Fore.BLUE + colorama.Style.BRIGHT + " Welcome to BootAnimix Shell, Type 'help' for more information" + "\n" + colorama.Style.RESET_ALL)
shell()
