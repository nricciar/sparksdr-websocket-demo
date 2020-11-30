import re
import os.path
from os import path
import json
import os
import csv

with open('lotw-user-activity.csv', newline='') as csvfile:
    reader = csv.reader(csvfile, delimiter=',', quotechar='"')
    filename = "lotw-users.dat"
    file1 = open(filename, "w")

    for row in reader:
        callsign = row[0]
        try:
            result = re.search('^[^\d]+\d{1}', callsign).group()
            prefix_dir = "../static/out/{prefix}".format(prefix=result)
            filename = "../static/out/{prefix}/{callsign}.json".format(prefix=result,callsign=callsign)
        except:
            print("Unable to parse callsign {callsign}".format(callsign=callsign))
            continue

        if not os.path.exists(filename):
            file1.write("{callsign}\n".format(callsign=callsign))

    file1.close()