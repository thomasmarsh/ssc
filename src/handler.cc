#include "handler.h"
#include "global.h"
#include "menu.h"

// ---------------------------------------------------------------------------
//
// PLAY HANDLER
//
// ---------------------------------------------------------------------------

PlayHandler::PlayHandler()
    : EventHandler()
{
}

extern bool accelFlag;
void PlayHandler::process(double dt)
{
    if (mController->isPaused())
        return;

    // don't process input if the player is not alive
    if (Global::ship->getState() != ScreenObject::ALIVE)
        return;

    // dispatch events
    if (mouseMotion()) {
        Global::ship->rotation = atan2((double)(mouseX() - (Screen::mDisplay.x >> 1)),
            (double)((Screen::mDisplay.y >> 1) - mouseY()));
        Global::ship->rotate(0);
    } else {
        if (keyPressed(SDLK_RIGHT))
            Global::ship->rotate(RAD(5) * dt);
        if (keyPressed(SDLK_LEFT))
            Global::ship->rotate(RAD(-5) * dt);
    }

    if (keyPressed(SDLK_UP) || mouseButton(1))
        Global::ship->accelerate(dt), accelFlag = true;
    else
        accelFlag = false;

    if (keyPressed(SDLK_DOWN) || mouseButton(3))
        Global::ship->decelerate(dt);

    if (keyPressed(SDLK_a)) {
        Global::ship->fire();
    }
}

// ---------------------------------------------------------------------------
//
// MENU HANDLER
//
// ---------------------------------------------------------------------------

MenuHandler::MenuHandler()
    : EventHandler()
    , mGameMenu(0)
{
}

static bool canmenu = true;
static double menutm = 0;

static bool mup = false, mdown = false, mleft = false, mright = false, mexec = false;

void MenuHandler::process(double dt)
{
    menutm += dt;
    if (menutm > 10) {
        menutm = 0;
        canmenu = true;
    }
    if (!canmenu)
        return;
    else if (mup)
        mGameMenu->moveUp();
    else if (mdown)
        mGameMenu->moveDown();
    else if (mleft)
        mGameMenu->moveLeft();
    else if (mright)
        mGameMenu->moveRight();
    else if (mexec)
        mGameMenu->exec(mController);

    mup = false, mdown = false, mexec = false, mleft = false, mright = false;
    canmenu = false;
}

void MenuHandler::pressKey(SDLKey c)
{
    mup = false, mdown = false, mexec = false, mleft = false, mright = false;
    switch (c) {
    case SDLK_RETURN:
    case SDLK_SPACE:
    case SDLK_KP_ENTER:
        mexec = true;
        break;
    case SDLK_UP:
        mup = true;
        break;
    case SDLK_DOWN:
        mdown = true;
        break;
    case SDLK_LEFT:
        mleft = true;
        break;
    case SDLK_RIGHT:
        mright = true;
        break;
    default:
        break;
    }
}
