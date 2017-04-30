#ifndef SSC_MENU_H
#define SSC_MENU_H

#include "control.h"

//! \enum MenuAction
//
//! Used to indicate the next state in the menu system.

enum MenuAction
{
        MENU_START,
        MENU_CONFIG,
        MENU_QUIT,
        MENU_MAIN
};

enum MenuModify
{
        MENU_M_NONE,
        MENU_M_RESOLUTION
};


//! \struct MenuItem
//
//! An individual item in the Menu.

struct MenuItem
{
        //! Text message associated with the current menu item
        const char *text;

        //! The action to perform when item is activated
        MenuAction action;

        //! What gets changed when the user presses left/right on the item
        MenuModify modify;
};


//! \class GameMenu
//
//! Draws and maintains the menu state.

class GameMenu
{
public:
        GameMenu();
        void draw();
        void exec(Controller *c);

        void moveUp();
        void moveDown();
        void moveLeft();
        void moveRight();
private:
        void clear();
        void addItem(const char *text, MenuAction action, MenuModify=MENU_M_NONE);
        unsigned int mSelected, mNumMenus;
        MenuItem mMenuItem[10];
};

#endif // SSC_MENU_H
