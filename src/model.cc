// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "model.h"
#include "hud.h"
#include "font.h"
#include "physics.h"
#include "game.h"

Model* Model::mInstance=0;
Environ *mEnviron;

unsigned int numDead = 0;
extern double mFramerate;

Model::Model() : mHead(0)
{
	(void) new Wall( 1,  0,  0,  0);
	(void) new Wall( 0,  1,  0,  0);
	(void) new Wall(-1,  0,  0, -Screen::maxX());
	(void) new Wall( 0, -1,  0, -Screen::maxY());
}

void Model::addObject(ScreenObject *obj)
{
	if (!mHead)
	{
		mHead = obj;
		mHead->prev = 0;
		mHead->next = 0;
		return;
	}
	ScreenObject *current = mHead;

	while (current->next)
		current = current->next;

	assert(current);
	current->next = obj;
	obj->prev = current;
	obj->next = 0;
}

struct Particle
{
	Coord3<double> mPosition, mForce;
	double r, g, b;
	double time;

	Particle() : mPosition(0, 0, 0), mForce(0, 0, 0), r(0), g(0), b(0), time(0) {}
};

const unsigned int MAXPARTICLES = 30000;

Particle mParticles[MAXPARTICLES];

unsigned int mParticleId = 0;

void addParticle(double x, double y, double z,
		 double fx, double fy, double fz,
		 double r, double g, double b,
		 double time)
{
	mParticles[mParticleId].mPosition.set(x, y, z);
	mParticles[mParticleId].mForce.set(fx, fy, fz);
	mParticles[mParticleId].r = r;
	mParticles[mParticleId].g = g;
	mParticles[mParticleId].b = b;
	mParticles[mParticleId].time = time;

	mParticleId++;
	if (mParticleId >= MAXPARTICLES)
		mParticleId = 0;
}


extern bool mDrawP;
inline
void drawParticles(double dt, bool doMove)
{
	if (!mDrawP) return;
	Coord3<double> pos;
	Particle *p, *lp;

	lp = &mParticles[MAXPARTICLES];

	draw::startPoints();
	for (p = mParticles; p < lp; p++)
	{
		if (p->time > 0)
		{
			if (doMove)
			{
				p->time -= dt;
				p->mPosition += (p->mForce * dt);
			}
			pos.set(p->mPosition);
			draw::setColor(p->r, p->g, p->b);
			draw::point((int) pos.x, (int) pos.y, (int) pos.z);
		}
	}
	draw::endPoints();
}


inline
void Model::draw(double dt, bool doMove)
{
	draw::clearScreen();
	glPushMatrix();

	//
	// point camera in right direction
	//

	double x = (Global::ship->mPosition.x),
	       y = -(Global::ship->mPosition.y);

	static Coord3<double> shipPos;

	shipPos.set(x, y, 0);
	mCamera.setTarget(shipPos, Global::ship->rotation, Global::ship->speed);
	mCamera.update(dt);

	//
	// draw the stars
	//

	mStarField.draw();
	draw::setColor(1, 0, 1, 1);

	//
	// draw the guide lines at the border of the gameplay area
	//

	int ax = 0, ay = 0, bx = Screen::maxX(), by = Screen::maxY();
	int extra = 1000;
	draw::line(ax, ay-extra, ax, by+extra);
	draw::line(ax-extra, by, bx+extra, by);
	draw::line(bx, by+extra, bx, ay-extra);
	draw::line(bx+extra, ay, ax-extra, ay);


	//
	// Call the particle engine draw method
	//

	drawParticles(dt, doMove);


	//
	// draw all screenobjects, removing any dead
	//

	Global::ship->sync();
	ScreenObject *i = mHead, *toDelete(0);
	while (i)
	{
		// delete dead objects
		if (i->isDead())
		{
			toDelete = i;

			// reset the prev/next links
			if (i->next)	i->next->prev = i->prev;
			if (i->prev)	i->prev->next = i->next;
			else		mHead = mHead->next;

			// set i to next value
			i = toDelete->next;

			delete toDelete;
			toDelete = 0;
		}
		else
		{
			// we only move if we're not paused and the object is
			// in a certain state
			if (doMove && ((i->isAlive()) || (i->isDying())))
				i->sync(), i->move(dt);

			// ...but we always draw
			i->draw();

			// set i to next value
			i = i->next;
		}
	}

	//
	// draw the HUD (Heads-Up-Display)
	//

	HUD::getInstance()->draw();

	static Game *game = Game::getInstance();

	if (game->getMode() == Game::MENU)
		game->drawMenu();

	glPopMatrix();
	draw::flipBuffers();
}


void Model::setPlayerPos()
{
	HUD::getInstance()->update(Global::ship);
}

void Model::startGame() { clearLevel(true); }

void Model::clearLevel(bool reset)
{
	ScreenObject *tmp;

	ScreenObject *i = mHead;

	while (i)
	{
		if (i->type() != ScreenObject::PLAYER_TYPE)
		{
			tmp = i;

			i->setState(ScreenObject::DYING);
			i->setState(ScreenObject::DEAD);

			if (i->next)	i->next->prev = i->prev;
			if (i->prev)	i->prev->next = i->next;
			else		mHead = mHead->next;

			i = tmp->next;

			delete tmp;
			tmp = 0;
		}
		else i = i->next;
	}
	if (reset)
		Global::ship->init();
}

void Model::update(double dt, bool doMove)
{
	Environ::getInstance()->update(dt);
	draw(dt, doMove);
//	draw::checkErrors();
}
