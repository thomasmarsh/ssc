for f in *.cc
do
    echo $f
    clang++ -c $f -I/usr/local/include -I/usr/local/include/freetype2 -w
done 
clang++ *.o -o ssc -lglpng -framework GLUT -framework OpenGL -framework Cocoa -lode -lSDL -lSDL_mixer -lfreetype  -lSDLmain
