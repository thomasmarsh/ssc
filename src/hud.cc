// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "hud.h"
#include "draw.h"
#include "graph.h"
#include "model.h"

HUD::HUD()
    : ship(0)
    , mShowRadar(false)
{
}

HUD* HUD::hud = 0;
const char* HUD::SPEED_TEXT = "Speed:";
const char* HUD::ANGLE_TEXT = "Angle:";

void HUD::update(Ship* s)
{
    ship = s;
}

void drawForce(int x, int y, int percent)
{
    draw::setColor(1, 1, 1, .2);
    draw::box(x, y, 110, y + 10);
    double strength = (double)percent / 100.0;
    draw::setColor(1 - strength, strength, 0);
    draw::box(x, y, x + percent, y + 10);
}

void HUD::draw()
{
    draw::setMode(draw::DRAW_2D);

    int xres = Screen::mDisplay.x,
        yres = Screen::mDisplay.y;

    draw::setColor(0, 0, .75, .4);
    draw::box(0, yres - 40, xres, yres);

    drawForce(10, yres - 31, (int)(ship->shield.getStrength() * 100));
    drawForce(10, yres - 19, (int)(ship->mLife * 100));

    if (mShowRadar) {
        // draw radar

        int height = 100,
            width = 100,
            offx = 10,
            offy = 10;

        // draw frame
        draw::setColor(1, 1, 1);
        draw::rect(offx - 1, offy - 1, offx + width + 1, offy + height + 1, 0, true);

        // draw background
        draw::setColor(0, 0, 0, 0.5f);
        draw::box(offx, offy, offx + width, offy + height);

        // draw points

        draw::startPoints();

        for (ScreenObject* i = Model::getInstance()->getHead(); i != 0; i = i->next) {
            if (i->isAlive()) {
                const ScreenObject& obj = *i;
                int x = (int)obj.mPosition.x,
                    y = (int)obj.mPosition.y;

                draw::setColor(obj.r, obj.g, obj.b);
                draw::point(offx + (int)((x / (double)Screen::maxX()) * width),
                    offy + (int)((y / (double)Screen::maxY()) * height));
            }
        }
        draw::endPoints();
    }
    draw::setMode(draw::DRAW_3D);
}

void HUD::initialize()
{
}
