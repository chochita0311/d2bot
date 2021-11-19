import cv2 as cv
import numpy as np

runepool_img = cv.imread('runepool.png', cv.IMREAD_UNCHANGED)
jahrune_img = cv.imread('jahrune.png', cv.IMREAD_UNCHANGED)

result = cv.matchTemplate(runepool_img, jahrune_img, cv.TM_CCOEFF_NORMED)

# get the best match position
min_val, max_val, min_loc, max_loc = cv.minMaxLoc(result)

print('Best match top left position: %s' % str(max_loc))
print('Best match confidence: %s' % max_val)

threshold = 0.8
if max_val >= threshold:
    print('Found jah.')

    jah_w = jahrune_img.shape[1]
    jah_h = jahrune_img.shape[0]

    top_left = max_loc
    bottom_right = (top_left[0] + jah_w, top_left[1] + jah_h)

    cv.rectangle(runepool_img, top_left, bottom_right,
                     color=(0, 255, 0), thickness=2, lineType=cv.LINE_4)
    cv.imshow('Result', result)
    cv.waitKey()
else:
    print('jah not found.')
