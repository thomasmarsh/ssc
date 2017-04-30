#include "menu.h"
#include "font.h"
#include "game.h"
#include "screen.h"

OGLFT::Monochrome* face = 0;

void DrawHeading()
{
    draw::setColor(0, 0, 0, .5);
    draw::box(0, 0, Screen::mDisplay.x, Screen::mDisplay.y);
    if (!face) {
        face = new OGLFT::Monochrome("/usr/local/share/ssc/Vera.ttf");
        if (!face || !face->isValid()) {
            fprintf(stderr, "Could not open Vera.ttf!\n");
            exit(0);
        }
    }

    face->setForegroundColor(1, 1, 1);
    face->setPointSize(24);
    face->draw(5, Screen::mDisplay.y - 30, "SSC: Strategic Space Combat");
}

GameMenu::GameMenu()
    : mSelected(0)
    , mNumMenus(0)
{
    mMenuItem[0].action = MENU_MAIN;
    exec(0);
}

void GameMenu::draw()
{
    draw::setMode(draw::DRAW_2D);
    DrawHeading();

    face->setPointSize(14);
    int xo = 100, yo = 150;

    for (unsigned int i = 0; i < mNumMenus; ++i) {
        if (mSelected == i) {
            face->setForegroundColor(1, 1, 0);
        } else {
            face->setForegroundColor(1, 1, 1);
        }

        face->draw(xo, Screen::mDisplay.y - yo - 20 * i,
            mMenuItem[i].text);
    }
    draw::setMode(draw::DRAW_3D);
}

void GameMenu::exec(Controller* c)
{
    switch (mMenuItem[mSelected].action) {
    case MENU_START:
        Game::getInstance()->setMode(Game::PLAY);
        break;
    case MENU_QUIT:
        c->setQuit();
        break;
    case MENU_CONFIG:
        clear();
        addItem("Game Mode", MENU_CONFIG);
        addItem("Difficulty", MENU_CONFIG);
        addItem("Control", MENU_CONFIG);
        addItem("Video", MENU_CONFIG, MENU_M_RESOLUTION);
        addItem("Audio", MENU_CONFIG);
        addItem("<return>", MENU_MAIN);
        break;
    case MENU_MAIN:
        clear();
        addItem("Play SSC", MENU_START);
        addItem("Configure", MENU_CONFIG);
        addItem("Exit Game", MENU_QUIT);
        break;
    }
}

void GameMenu::moveUp()
{
    if (mSelected == 0)
        mSelected = mNumMenus - 1;
    else
        mSelected--;
}

void GameMenu::moveDown()
{
    if (mSelected == (mNumMenus - 1)) {
        mSelected = 0;
    } else {
        mSelected++;
    }
}

void GameMenu::moveLeft()
{
    switch (mMenuItem[mSelected].modify) {
    case MENU_M_NONE:
        break;
    case MENU_M_RESOLUTION:
        // XXX: increase resolution
        break;
    }
}

void GameMenu::moveRight()
{
    switch (mMenuItem[mSelected].modify) {
    case MENU_M_NONE:
        break;
    case MENU_M_RESOLUTION:
        // XXX: decrease resolution
        break;
    }
}

void GameMenu::clear()
{
    mSelected = 0;
    mNumMenus = 0;
}

void GameMenu::addItem(const char* text, MenuAction action, MenuModify modify)
{
    mMenuItem[mNumMenus].text = text;
    mMenuItem[mNumMenus].action = action;
    mMenuItem[mNumMenus].modify = modify;
    mNumMenus++;
}
